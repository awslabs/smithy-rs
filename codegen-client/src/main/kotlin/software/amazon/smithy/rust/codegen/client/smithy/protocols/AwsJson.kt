/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.protocols

import software.amazon.smithy.model.Model
import software.amazon.smithy.model.pattern.UriPattern
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ToShapeId
import software.amazon.smithy.model.traits.HttpTrait
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.client.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.client.rustlang.RustModule
import software.amazon.smithy.rust.codegen.client.rustlang.asType
import software.amazon.smithy.rust.codegen.client.rustlang.rust
import software.amazon.smithy.rust.codegen.client.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.client.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.client.rustlang.writable
import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.CoreCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.client.smithy.generators.protocol.ProtocolSupport
import software.amazon.smithy.rust.codegen.client.smithy.generators.serializationError
import software.amazon.smithy.rust.codegen.client.smithy.protocols.parse.JsonParserGenerator
import software.amazon.smithy.rust.codegen.client.smithy.protocols.parse.StructuredDataParserGenerator
import software.amazon.smithy.rust.codegen.client.smithy.protocols.serialize.JsonSerializerGenerator
import software.amazon.smithy.rust.codegen.client.smithy.protocols.serialize.StructuredDataSerializerGenerator
import software.amazon.smithy.rust.codegen.client.util.inputShape

sealed class AwsJsonVersion {
    abstract val value: String

    object Json10 : AwsJsonVersion() {
        override val value = "1.0"
    }

    object Json11 : AwsJsonVersion() {
        override val value = "1.1"
    }
}

class AwsJsonFactory(private val version: AwsJsonVersion) : ProtocolGeneratorFactory<HttpBoundProtocolGenerator, ClientCodegenContext> {
    override fun protocol(codegenContext: ClientCodegenContext): Protocol = AwsJson(codegenContext, version)

    override fun buildProtocolGenerator(codegenContext: ClientCodegenContext): HttpBoundProtocolGenerator =
        HttpBoundProtocolGenerator(codegenContext, protocol(codegenContext))

    override fun support(): ProtocolSupport = ProtocolSupport(
        /* Client support */
        requestSerialization = true,
        requestBodySerialization = true,
        responseDeserialization = true,
        errorDeserialization = true,
        /* Server support */
        requestDeserialization = false,
        requestBodyDeserialization = false,
        responseSerialization = false,
        errorSerialization = false,
    )
}

class AwsJsonHttpBindingResolver(
    private val model: Model,
    private val awsJsonVersion: AwsJsonVersion,
) : HttpBindingResolver {
    private val httpTrait = HttpTrait.builder()
        .code(200)
        .method("POST")
        .uri(UriPattern.parse("/"))
        .build()

    private fun bindings(shape: ToShapeId) =
        shape.let { model.expectShape(it.toShapeId()) }.members()
            .map { HttpBindingDescriptor(it, HttpLocation.DOCUMENT, "document") }
            .toList()

    override fun httpTrait(operationShape: OperationShape): HttpTrait = httpTrait

    override fun requestBindings(operationShape: OperationShape): List<HttpBindingDescriptor> =
        bindings(operationShape.inputShape)

    override fun responseBindings(operationShape: OperationShape): List<HttpBindingDescriptor> =
        bindings(operationShape.outputShape)

    override fun errorResponseBindings(errorShape: ToShapeId): List<HttpBindingDescriptor> =
        bindings(errorShape)

    override fun requestContentType(operationShape: OperationShape): String =
        "application/x-amz-json-${awsJsonVersion.value}"

    override fun responseContentType(operationShape: OperationShape): String = requestContentType(operationShape)
}

/**
 * AwsJson requires all empty inputs to be sent across as `{}`. This class
 * customizes wraps [JsonSerializerGenerator] to add this functionality.
 */
class AwsJsonSerializerGenerator(
    private val coreCodegenContext: CoreCodegenContext,
    httpBindingResolver: HttpBindingResolver,
    private val jsonSerializerGenerator: JsonSerializerGenerator =
        JsonSerializerGenerator(coreCodegenContext, httpBindingResolver, ::awsJsonFieldName),
) : StructuredDataSerializerGenerator by jsonSerializerGenerator {
    private val runtimeConfig = coreCodegenContext.runtimeConfig
    private val codegenScope = arrayOf(
        "Error" to runtimeConfig.serializationError(),
        "SdkBody" to RuntimeType.sdkBody(runtimeConfig),
    )

    override fun operationInputSerializer(operationShape: OperationShape): RuntimeType {
        var serializer = jsonSerializerGenerator.operationInputSerializer(operationShape)
        if (serializer == null) {
            val inputShape = operationShape.inputShape(coreCodegenContext.model)
            val fnName = coreCodegenContext.symbolProvider.serializeFunctionName(operationShape)
            serializer = RuntimeType.forInlineFun(fnName, RustModule.private("operation_ser")) {
                it.rustBlockTemplate(
                    "pub fn $fnName(_input: &#{target}) -> Result<#{SdkBody}, #{Error}>",
                    *codegenScope, "target" to coreCodegenContext.symbolProvider.toSymbol(inputShape),
                ) {
                    rustTemplate("""Ok(#{SdkBody}::from("{}"))""", *codegenScope)
                }
            }
        }
        return serializer
    }
}

open class AwsJson(
    private val coreCodegenContext: CoreCodegenContext,
    private val awsJsonVersion: AwsJsonVersion,
) : Protocol {
    private val runtimeConfig = coreCodegenContext.runtimeConfig
    private val errorScope = arrayOf(
        "Bytes" to RuntimeType.Bytes,
        "Error" to RuntimeType.GenericError(runtimeConfig),
        "HeaderMap" to RuntimeType.http.member("HeaderMap"),
        "JsonError" to CargoDependency.smithyJson(runtimeConfig).asType().member("deserialize::Error"),
        "Response" to RuntimeType.http.member("Response"),
        "json_errors" to RuntimeType.jsonErrors(runtimeConfig),
    )
    private val jsonDeserModule = RustModule.private("json_deser")

    override val httpBindingResolver: HttpBindingResolver =
        AwsJsonHttpBindingResolver(coreCodegenContext.model, awsJsonVersion)

    override val defaultTimestampFormat: TimestampFormatTrait.Format = TimestampFormatTrait.Format.EPOCH_SECONDS

    override fun additionalRequestHeaders(operationShape: OperationShape): List<Pair<String, String>> =
        listOf("x-amz-target" to "${coreCodegenContext.serviceShape.id.name}.${operationShape.id.name}")

    override fun structuredDataParser(operationShape: OperationShape): StructuredDataParserGenerator =
        JsonParserGenerator(coreCodegenContext, httpBindingResolver, ::awsJsonFieldName)

    override fun structuredDataSerializer(operationShape: OperationShape): StructuredDataSerializerGenerator =
        AwsJsonSerializerGenerator(coreCodegenContext, httpBindingResolver)

    override fun parseHttpGenericError(operationShape: OperationShape): RuntimeType =
        RuntimeType.forInlineFun("parse_http_generic_error", jsonDeserModule) { writer ->
            writer.rustTemplate(
                """
                pub fn parse_http_generic_error(response: &#{Response}<#{Bytes}>) -> Result<#{Error}, #{JsonError}> {
                    #{json_errors}::parse_generic_error(response.body(), response.headers())
                }
                """,
                *errorScope,
            )
        }

    override fun parseEventStreamGenericError(operationShape: OperationShape): RuntimeType =
        RuntimeType.forInlineFun("parse_event_stream_generic_error", jsonDeserModule) { writer ->
            writer.rustTemplate(
                """
                pub fn parse_event_stream_generic_error(payload: &#{Bytes}) -> Result<#{Error}, #{JsonError}> {
                    // Note: HeaderMap::new() doesn't allocate
                    #{json_errors}::parse_generic_error(payload, &#{HeaderMap}::new())
                }
                """,
                *errorScope,
            )
        }

    /**
     * Returns the operation name as required by the awsJson1.x protocols.
     */
    override fun serverRouterRequestSpec(
        operationShape: OperationShape,
        operationName: String,
        serviceName: String,
        requestSpecModule: RuntimeType,
    ) = writable {
        rust("""String::from("$serviceName.$operationName")""")
    }

    override fun serverRouterRuntimeConstructor() = when (awsJsonVersion) {
        AwsJsonVersion.Json10 -> "new_aws_json_10_router"
        AwsJsonVersion.Json11 -> "new_aws_json_11_router"
    }
}

fun awsJsonFieldName(member: MemberShape): String = member.memberName
