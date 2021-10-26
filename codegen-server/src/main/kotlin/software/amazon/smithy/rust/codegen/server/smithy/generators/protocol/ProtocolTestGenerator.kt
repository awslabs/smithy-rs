/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators.protocol

import software.amazon.smithy.codegen.core.CodegenException
import software.amazon.smithy.model.knowledge.OperationIndex
import software.amazon.smithy.model.shapes.DoubleShape
import software.amazon.smithy.model.shapes.FloatShape
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.traits.ErrorTrait
import software.amazon.smithy.model.traits.IdempotencyTokenTrait
import software.amazon.smithy.protocoltests.traits.AppliesTo
import software.amazon.smithy.protocoltests.traits.HttpMessageTestCase
import software.amazon.smithy.protocoltests.traits.HttpRequestTestCase
import software.amazon.smithy.protocoltests.traits.HttpRequestTestsTrait
import software.amazon.smithy.protocoltests.traits.HttpResponseTestCase
import software.amazon.smithy.protocoltests.traits.HttpResponseTestsTrait
import software.amazon.smithy.rust.codegen.rustlang.Attribute
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.CratesIo
import software.amazon.smithy.rust.codegen.rustlang.DependencyScope
import software.amazon.smithy.rust.codegen.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.withBlock
import software.amazon.smithy.rust.codegen.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.generators.Instantiator
import software.amazon.smithy.rust.codegen.smithy.generators.error.errorSymbol
import software.amazon.smithy.rust.codegen.smithy.generators.protocol.ProtocolSupport
import software.amazon.smithy.rust.codegen.util.dq
import software.amazon.smithy.rust.codegen.util.findMemberWithTrait
import software.amazon.smithy.rust.codegen.util.getTrait
import software.amazon.smithy.rust.codegen.util.hasTrait
import software.amazon.smithy.rust.codegen.util.inputShape
import software.amazon.smithy.rust.codegen.util.isStreaming
import software.amazon.smithy.rust.codegen.util.orNull
import software.amazon.smithy.rust.codegen.util.outputShape
import software.amazon.smithy.rust.codegen.util.toSnakeCase
import java.util.logging.Logger

/**
 * Generate protocol tests for an operation
 */
class ProtocolTestGenerator(
    private val codegenContext: CodegenContext,
    private val protocolSupport: ProtocolSupport,
    private val operationShape: OperationShape,
    private val writer: RustWriter
) {
    private val logger = Logger.getLogger(javaClass.name)

    private val inputShape = operationShape.inputShape(codegenContext.model)
    private val outputShape = operationShape.outputShape(codegenContext.model)
    private val operationSymbol = codegenContext.symbolProvider.toSymbol(operationShape)
    private val operationIndex = OperationIndex.of(codegenContext.model)
    private val operationSerType = RuntimeType("operation_ser", null, "crate")

    private val instantiator = with(codegenContext) {
        Instantiator(symbolProvider, model, runtimeConfig)
    }

    sealed class TestCase {
        abstract val testCase: HttpMessageTestCase

        data class RequestTest(override val testCase: HttpRequestTestCase, val targetShape: StructureShape) :
            TestCase()
        data class ResponseTest(override val testCase: HttpResponseTestCase, val targetShape: StructureShape) :
            TestCase()
    }

    fun render() {
        val requestTests = operationShape.getTrait<HttpRequestTestsTrait>()
            ?.getTestCasesFor(AppliesTo.SERVER).orEmpty().map { TestCase.RequestTest(it, inputShape) }
        val responseTests = operationShape.getTrait<HttpResponseTestsTrait>()
            ?.getTestCasesFor(AppliesTo.SERVER).orEmpty().map { TestCase.ResponseTest(it, outputShape) }

        val errorTests = operationIndex.getErrors(operationShape).flatMap { error ->
            val testCases = error.getTrait<HttpResponseTestsTrait>()?.testCases.orEmpty()
            testCases.map { TestCase.ResponseTest(it, error) }
        }
        val allTests: List<TestCase> = (requestTests + responseTests + errorTests).filterMatching()
        if (allTests.isNotEmpty()) {
            val operationName = operationSymbol.name
            val testModuleName = "server_${operationName.toSnakeCase()}_test"
            val moduleMeta = RustMetadata(
                public = false,
                additionalAttributes = listOf(
                    Attribute.Cfg("test"),
                    Attribute.Custom("allow(unreachable_code, unused_variables)")
                )
            )
            writer.withModule(testModuleName, moduleMeta) {
                renderAllTestCases(allTests)
            }
        }
    }

    private fun RustWriter.renderAllTestCases(allTests: List<TestCase>) {
        allTests.forEach {
            renderTestCaseBlock(it.testCase, this) {
                when (it) {
                    is TestCase.RequestTest -> this.renderHttpRequestTestCase(it.testCase, it.targetShape)
                    is TestCase.ResponseTest -> this.renderHttpResponseTestCase(it.testCase, it.targetShape)
                }
            }
        }
    }

    /**
     * Filter out test cases that are disabled or don't match the service protocol
     */
    private fun List<TestCase>.filterMatching(): List<TestCase> {
        return if (RunOnly.isNullOrEmpty()) {
            this.filter { testCase ->
                testCase.testCase.protocol == codegenContext.protocol &&
                    !DisableTests.contains(testCase.testCase.id)
            }
        } else {
            this.filter { RunOnly.contains(it.testCase.id) }
        }
    }

    private fun renderTestCaseBlock(
        testCase: HttpMessageTestCase,
        testModuleWriter: RustWriter,
        block: RustWriter.() -> Unit
    ) {
        testModuleWriter.setNewlinePrefix("/// ")
        testCase.documentation.map {
            testModuleWriter.writeWithNoFormatting(it)
        }
        testModuleWriter.write("Test ID: ${testCase.id}")
        testModuleWriter.setNewlinePrefix("")
        testModuleWriter.writeWithNoFormatting("#[tokio::test]")
        val Tokio = CargoDependency(
            "tokio",
            CratesIo("1"),
            features = setOf("macros", "test-util", "rt"),
            scope = DependencyScope.Dev
        )
        testModuleWriter.addDependency(Tokio)
        val action = when (testCase) {
            is HttpResponseTestCase -> Action.Response
            is HttpRequestTestCase -> Action.Request
            else -> throw CodegenException("unknown test case type")
        }
        if (expectFail(testCase)) {
            testModuleWriter.writeWithNoFormatting("#[should_panic]")
        }
        val fnName = when (action) {
            is Action.Response -> "_response"
            is Action.Request -> "_request"
        }
        testModuleWriter.rustBlock("async fn ${testCase.id.toSnakeCase()}$fnName()") {
            block(this)
        }
    }

    private fun RustWriter.renderHttpRequestTestCase(
        httpRequestTestCase: HttpRequestTestCase,
        inputShape: StructureShape,
    ) {
        if (!protocolSupport.requestDeserialization) {
            rust("/* test case disabled for this protocol (not yet supported) */")
            return
        }
        val customToken = if (inputShape.findMemberWithTrait<IdempotencyTokenTrait>(codegenContext.model) != null) {
            """.make_token("00000000-0000-4000-8000-000000000000")"""
        } else ""
        rust(
            """let config = #T::Config::builder()$customToken.build();""",
            RuntimeType.Config
        )
        writeInline("let expected =")
        instantiator.render(this, inputShape, httpRequestTestCase.params)
        write(";")
        rust("""let op = expected.make_operation(&config).await.expect("operation failed to build");""")
        rust("let (http_request, parts) = op.into_request_response().0.into_parts();")
        with(httpRequestTestCase) {
            host.orNull()?.also { host ->
                val withScheme = "http://$host"
                rust(
                    """
                    let mut http_request = http_request;
                    let ep = #T::endpoint::Endpoint::mutable(#T::Uri::from_static(${withScheme.dq()}));
                    ep.set_endpoint(http_request.uri_mut(), parts.acquire().get());
                """,
                    CargoDependency.SmithyHttp(codegenContext.runtimeConfig).asType(), CargoDependency.Http.asType()
                )
            }
            rust(
                """
                    assert_eq!(http_request.method(), ${method.dq()});
                    assert_eq!(http_request.uri().path(), ${uri.dq()});
                """
            )
            resolvedHost.orNull()?.also { host ->
                rust("""assert_eq!(http_request.uri().host().expect("host should be set"), ${host.dq()});""")
            }
        }
        checkQueryParams(this, httpRequestTestCase.queryParams)
        checkForbidQueryParams(this, httpRequestTestCase.forbidQueryParams)
        checkRequiredQueryParams(this, httpRequestTestCase.requireQueryParams)
        checkHeaders(this, httpRequestTestCase.headers)
        checkForbidHeaders(this, httpRequestTestCase.forbidHeaders)
        checkRequiredHeaders(this, httpRequestTestCase.requireHeaders)
        if (protocolSupport.requestBodyDeserialization) {
            // "If no request body is defined, then no assertions are made about the body of the message."
            httpRequestTestCase.body.orNull()?.also { body ->
                checkBody(this, body)
            }
        }

        // Explicitly warn if the test case defined parameters that we aren't doing anything with
        with(httpRequestTestCase) {
            if (authScheme.isPresent) {
                logger.warning("Test case provided authScheme but this was ignored")
            }
            if (!httpRequestTestCase.vendorParams.isEmpty) {
                logger.warning("Test case provided vendorParams but these were ignored")
            }
        }
    }

    private fun HttpMessageTestCase.action(): Action = when (this) {
        is HttpRequestTestCase -> Action.Request
        is HttpResponseTestCase -> Action.Response
        else -> throw CodegenException("Unknown test case type")
    }

    private fun expectFail(testCase: HttpMessageTestCase): Boolean = ExpectFail.find {
        it.id == testCase.id && it.action == testCase.action() && it.service == codegenContext.serviceShape.id.toString()
    } != null

    private fun RustWriter.renderHttpResponseTestCase(
        testCase: HttpResponseTestCase,
        expectedShape: StructureShape
    ) {
        if (!protocolSupport.responseSerialization || (
            !protocolSupport.errorSerialization && expectedShape.hasTrait(
                    ErrorTrait::class.java
                )
            )
        ) {
            rust("/* test case disabled for this protocol (not yet supported) */")
            return
        }
        val fnName = if (expectedShape.hasTrait<ErrorTrait>())
            "serialize_${operationShape.id.name.toSnakeCase()}_error"
        else
            "serialize_${operationShape.id.name.toSnakeCase()}_response"
        writeInline("let expected_output =")
        instantiator.render(this, expectedShape, testCase.params)
        write(";")
        rustTemplate(
            """
            use #{ParseStrictResponse};
            use #{SerializeStrictResponse};
            let op = #{op}::new();
            let http_response = op.serialize_response(&expected_output).expect("unable to serialize response");
            let parsed = op.parse(&http_response);
        """,
            "op" to operationSymbol,
            "ParseStrictResponse" to RuntimeType.parseStrictResponse(codegenContext.runtimeConfig),
            "SerializeStrictResponse" to RuntimeType.serializeStrictResponse(codegenContext.runtimeConfig),
        )
        if (expectedShape.hasTrait<ErrorTrait>()) {
            val errorSymbol = operationShape.errorSymbol(codegenContext.symbolProvider)
            val errorVariant = codegenContext.symbolProvider.toSymbol(expectedShape).name
            rust("""let parsed = parsed.expect_err("should be error response");""")
            rustBlock("if let #TKind::$errorVariant(actual_error) = parsed.kind", errorSymbol) {
                rust("assert_eq!(expected_output, actual_error);")
            }
            rustBlock("else") {
                rust("panic!(\"wrong variant: Got: {:?}. Expected: {:?}\", parsed, expected_output);")
            }
        } else {
            rust("let parsed = parsed.unwrap();")
            outputShape.members().forEach { member ->
                val memberName = codegenContext.symbolProvider.toMemberName(member)
                if (member.isStreaming(codegenContext.model)) {
                    rust(
                        """assert_eq!(
                                        parsed.$memberName.collect().await.unwrap().into_bytes(),
                                        expected_output.$memberName.collect().await.unwrap().into_bytes()
                                    );"""
                    )
                } else {
                    when (codegenContext.model.expectShape(member.target)) {
                        is DoubleShape, is FloatShape -> {
                            addUseImports(
                                RuntimeType.ProtocolTestHelper(codegenContext.runtimeConfig, "FloatEquals").toSymbol()
                            )
                            rust(
                                """
                                assert!(parsed.$memberName.float_equals(&expected_output.$memberName),
                                    "Unexpected value for `$memberName` {:?} vs. {:?}", expected_output.$memberName, parsed.$memberName);
                                """
                            )
                        }
                        else ->
                            rust("""assert_eq!(parsed.$memberName, expected_output.$memberName, "Unexpected value for `$memberName`");""")
                    }
                }
            }
        }
    }

    private fun checkRequiredHeaders(rustWriter: RustWriter, requireHeaders: List<String>) {
        basicCheck(requireHeaders, rustWriter, "required_headers", "require_headers")
    }

    private fun checkForbidHeaders(rustWriter: RustWriter, forbidHeaders: List<String>) {
        basicCheck(forbidHeaders, rustWriter, "forbidden_headers", "forbid_headers")
    }

    private fun checkBody(rustWriter: RustWriter, body: String) {
        rustWriter.write(
            """let http_request = http_request.map(|body| #T::from(body.bytes().unwrap().to_vec()));""",
            RuntimeType.Bytes
        )
        rustWriter.rustTemplate(
            """
            use #{parse_strict_request};
            let op = #{op}::new();
            let body = op.parse(&http_request).expect("failed to parse request");
            """,
            "op" to operationSymbol,
            "parse_strict_request" to CargoDependency.SmithyHttp(codegenContext.runtimeConfig).asType().member("request::ParseStrictRequest"),
        )
        if (body == "") {
            rustWriter.write("// No body")
            rustWriter.write("assert_eq!(std::str::from_utf8(body).unwrap(), ${"".dq()});")
        } else {
            rustWriter.write("assert_eq!(expected, body);")
        }
    }

    private fun checkHeaders(rustWriter: RustWriter, headers: Map<String, String>) {
        if (headers.isEmpty()) {
            return
        }
        val variableName = "expected_headers"
        rustWriter.withBlock("let $variableName = &[", "];") {
            write(
                headers.entries.joinToString(",") {
                    "(${it.key.dq()}, ${it.value.dq()})"
                }
            )
        }
        assertOk(rustWriter) {
            write(
                "#T(&http_request, $variableName)",
                RuntimeType.ProtocolTestHelper(codegenContext.runtimeConfig, "validate_headers")
            )
        }
    }

    private fun checkRequiredQueryParams(
        rustWriter: RustWriter,
        requiredParams: List<String>
    ) = basicCheck(requiredParams, rustWriter, "required_params", "require_query_params")

    private fun checkForbidQueryParams(
        rustWriter: RustWriter,
        forbidParams: List<String>
    ) = basicCheck(forbidParams, rustWriter, "forbid_params", "forbid_query_params")

    private fun checkQueryParams(
        rustWriter: RustWriter,
        queryParams: List<String>
    ) = basicCheck(queryParams, rustWriter, "expected_query_params", "validate_query_string")

    private fun basicCheck(
        params: List<String>,
        rustWriter: RustWriter,
        variableName: String,
        checkFunction: String
    ) {
        if (params.isEmpty()) {
            return
        }
        rustWriter.withBlock("let $variableName = ", ";") {
            strSlice(this, params)
        }
        assertOk(rustWriter) {
            write(
                "#T(&http_request, $variableName)",
                RuntimeType.ProtocolTestHelper(codegenContext.runtimeConfig, checkFunction)
            )
        }
    }

    /**
     * wraps `inner` in a call to `aws_smithy_protocol_test::assert_ok`, a convenience wrapper
     * for pretty prettying protocol test helper results
     */
    private fun assertOk(rustWriter: RustWriter, inner: RustWriter.() -> Unit) {
        rustWriter.write("#T(", RuntimeType.ProtocolTestHelper(codegenContext.runtimeConfig, "assert_ok"))
        inner(rustWriter)
        rustWriter.write(");")
    }

    private fun strSlice(writer: RustWriter, args: List<String>) {
        writer.withBlock("&[", "]") {
            write(args.joinToString(",") { it.dq() })
        }
    }

    companion object {
        sealed class Action {
            object Request : Action()
            object Response : Action()
        }

        data class FailingTest(val service: String, val id: String, val action: Action)

        // These tests fail due to shortcomings in our implementation.
        // These could be configured via runtime configuration, but since this won't be long-lasting,
        // it makes sense to do the simplest thing for now.
        // The test will _fail_ if these pass, so we will discover & remove if we fix them by accident
        private val JsonRpc10 = "aws.protocoltests.json10#JsonRpc10"
        private val AwsJson11 = "aws.protocoltests.json#JsonProtocol"
        private val RestJson = "aws.protocoltests.restjson#RestJson"
        private val RestXml = "aws.protocoltests.restxml#RestXml"
        private val AwsQuery = "aws.protocoltests.query#AwsQuery"
        private val Ec2Query = "aws.protocoltests.ec2#AwsEc2"
        private val ExpectFail = setOf<FailingTest>()
        private val RunOnly: Set<String>? = null

        // These tests are not even attempted to be generated, either because they will not compile
        // or because they are flaky
        private val DisableTests = setOf<String>()
    }
}
