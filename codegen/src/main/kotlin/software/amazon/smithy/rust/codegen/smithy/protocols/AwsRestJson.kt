/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.protocols

import software.amazon.smithy.codegen.core.CodegenException
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.knowledge.HttpBinding
import software.amazon.smithy.model.knowledge.HttpBindingIndex
import software.amazon.smithy.model.shapes.BlobShape
import software.amazon.smithy.model.shapes.DocumentShape
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.EnumTrait
import software.amazon.smithy.model.traits.HttpTrait
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.rustlang.Attribute
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.withBlock
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.smithy.generators.HttpProtocolGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolConfig
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolGeneratorFactory
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolSupport
import software.amazon.smithy.rust.codegen.smithy.generators.StructureGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.builderSymbol
import software.amazon.smithy.rust.codegen.smithy.generators.errorSymbol
import software.amazon.smithy.rust.codegen.smithy.generators.http.RequestBindingGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.http.ResponseBindingGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.setterName
import software.amazon.smithy.rust.codegen.smithy.isOptional
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticOutputTrait
import software.amazon.smithy.rust.codegen.smithy.transformers.OperationNormalizer
import software.amazon.smithy.rust.codegen.util.dq
import software.amazon.smithy.rust.codegen.util.expectMember
import software.amazon.smithy.rust.codegen.util.outputShape
import java.util.logging.Logger

class AwsRestJsonFactory : ProtocolGeneratorFactory<AwsRestJsonGenerator> {
    override fun buildProtocolGenerator(
        protocolConfig: ProtocolConfig
    ): AwsRestJsonGenerator = AwsRestJsonGenerator(protocolConfig)

    /** Create a synthetic awsJsonInputBody if specified
     * A body is created if any member of [shape] is bound to the `DOCUMENT` section of the `bindings.
     */
    private fun restJsonBody(shape: StructureShape?, bindings: Map<String, HttpBinding>): StructureShape? {
        if (shape == null) {
            return null
        }
        val bodyMembers = shape.members().filter { member ->
            bindings[member.memberName]?.location == HttpBinding.Location.DOCUMENT
        }

        return if (bodyMembers.isNotEmpty()) {
            shape.toBuilder().members(bodyMembers).build()
        } else {
            null
        }
    }

    override fun transformModel(model: Model): Model {
        val httpIndex = HttpBindingIndex.of(model)
        return OperationNormalizer(model).transformModel(
            inputBodyFactory = { op, input -> restJsonBody(input, httpIndex.getRequestBindings(op)) },
            outputBodyFactory = { op, output -> restJsonBody(output, httpIndex.getResponseBindings(op)) },
        )
    }

    override fun support(): ProtocolSupport {
        // TODO: Support body for RestJson
        return ProtocolSupport(
            requestBodySerialization = true,
            responseDeserialization = true,
            errorDeserialization = true
        )
    }

    override fun symbolProvider(model: Model, base: RustSymbolProvider): RustSymbolProvider {
        return JsonSerializerSymbolProvider(
            model,
            SyntheticBodySymbolProvider(model, base),
            TimestampFormatTrait.Format.EPOCH_SECONDS
        )
    }
}

class AwsRestJsonGenerator(
    private val protocolConfig: ProtocolConfig
) : HttpProtocolGenerator(protocolConfig) {
    // restJson1 requires all operations to use the HTTP trait
    private val logger = Logger.getLogger(javaClass.name)

    private val model = protocolConfig.model
    override fun traitImplementations(operationWriter: RustWriter, operationShape: OperationShape) {
        // TODO: Implement parsing traits for AwsRestJson
    }

    override fun fromResponseImpl(implBlockWriter: RustWriter, operationShape: OperationShape) {
        val outputShape = operationShape.outputShape(model)
        val httpTrait = operationShape.expectTrait(HttpTrait::class.java)
        val bodyId = outputShape.expectTrait(SyntheticOutputTrait::class.java).body
        val bodyShape = bodyId?.let { model.expectShape(bodyId, StructureShape::class.java) }
        val errorSymbol = operationShape.errorSymbol(symbolProvider)
        val httpBindingGenerator = ResponseBindingGenerator(protocolConfig, operationShape)

        // Parsing the response works in two phases:
        // 1. Code generate "parse_xyz" methods for each field
        // 2. Code generate a parse_response method which utilizes the parse_xyz methods to set fields on a builder
        val parseFunctions: Map<String, String> =
            renderParseFunctions(
                operationShape.outputShape(model),
                httpIndex.getResponseBindings(operationShape),
                errorSymbol,
                httpBindingGenerator,
                implBlockWriter,
                bodyShape
            )

        val errorParsers: Map<ShapeId, Map<String, String>> = operationShape.errors.map { shapeId ->
            val errorShape = model.expectShape(shapeId, StructureShape::class.java)
            shapeId to renderParseFunctions(
                errorShape,
                httpIndex.getResponseBindings(errorShape),
                errorSymbol,
                httpBindingGenerator,
                implBlockWriter,
                errorShape
            )
        }.toMap()

        val jsonErrors = RuntimeType.awsJsonErrors(runtimeConfig)

        fromResponseFun(implBlockWriter, operationShape) {
            rustBlock("if #T::is_error(&response) && response.status().as_u16() != ${httpTrait.code}", jsonErrors) {
                rustTemplate(
                    """
                    let body = #{sj}::from_slice(response.body().as_ref())
                        .unwrap_or_else(|_|#{sj}::json!({}));
                    let generic = #{aws_json_errors}::parse_generic_error(&response, &body);
                    """,
                    "aws_json_errors" to jsonErrors, "sj" to RuntimeType.SJ
                )
                if (operationShape.errors.isNotEmpty()) {
                    rustTemplate(
                        """

                    let error_code = match generic.code() {
                        Some(code) => code,
                        None => return Err(#{error_symbol}::unhandled(generic))
                    };""",
                        "error_symbol" to errorSymbol
                    )
                    withBlock("return Err(match error_code {", "})") {
                        // approx:
                        /*
                            match error_code {
                                "Code1" => deserialize<Code1>(body),
                                "Code2" => deserialize<Code2>(body)
                            }
                         */
                        parseErrorVariants(operationShape, errorSymbol, errorParsers)
                    }
                } else {
                    write("return Err(#T::generic(generic))", errorSymbol)
                }
            }
            // avoid non-usage warnings
            withBlock("Ok({", "})") {
                renderShapeParser(outputShape, bodyShape, errorSymbol, parseFunctions)
            }
        }
    }

    private fun RustWriter.renderShapeParser(
        outputShape: StructureShape,
        bodyShape: StructureShape?,
        errorSymbol: RuntimeType,
        parseFunctions: Map<String, String>
    ) {
        Attribute.AllowUnusedMut.render(this)
        rust("let mut output = #T::default();", outputShape.builderSymbol(symbolProvider))
        rust("let _ = response;")
        if (bodyShape != null && parseFunctions.isNotEmpty()) {
            rustTemplate(
                """
                    let body_slice = response.body().as_ref();

                    let parsed_body: #{body} = if body_slice.is_empty() {
                        #{from_slice}(b"{}").map_err(#{err_symbol}::unhandled)?
                    } else {
                        #{from_slice}(response.body().as_ref()).map_err(#{err_symbol}::unhandled)?
                    };
                """,
                "body" to symbolProvider.toSymbol(bodyShape),
                "from_slice" to RuntimeType.SerdeJson("from_slice"),
                "err_symbol" to errorSymbol
            )
            Attribute.AllowUnusedMut.render(this)
            rust("let mut output = #T::default();", outputShape.builderSymbol(symbolProvider))
            rust("let _ = response;")
            if (bodyShape != null) {
                rustTemplate(
                    """
                let body_slice = response.body().as_ref();

                let parsed_body: #{body} = if body_slice.is_empty() {
                    #{body}::default()
                } else {
                    #{from_slice}(response.body().as_ref()).map_err(#{err_symbol}::unhandled)?
                };
            """,
                    "body" to symbolProvider.toSymbol(bodyShape),
                    "from_slice" to RuntimeType.SerdeJson("from_slice"),
                    "err_symbol" to errorSymbol
                )
            }
            outputShape.members().forEach { member ->
                val parsedValue = parseFunctions[member.memberName]
                    ?: throw CodegenException("No parser defined for $member!. This is a bug")
                // can delete when we don't have `todo!()` here anymore
                Attribute.Custom("allow(unreachable_code, clippy::diverging_sub_expression)").render(this)
                rust("{ output = output.${member.setterName()}($parsedValue); }")
            }

            val err = if (StructureGenerator.fallibleBuilder(outputShape, symbolProvider)) {
                ".map_err(|s|${format(errorSymbol)}::unhandled(s))?"
            } else ""
            rust("Ok(output.build()$err)")
        }
        outputShape.members().forEach { member ->
            val parsedValue = parseFunctions[member.memberName]
                ?: throw CodegenException("No parser defined for $member!. This is a bug")
            // can delete when we don't have `todo!()` here anymore
            Attribute.Custom("allow(unreachable_code, clippy::diverging_sub_expression)").render(this)
            rust("{ output = output.${member.setterName()}($parsedValue); }")
        }

        val err = if (StructureGenerator.fallibleBuilder(outputShape, symbolProvider)) {
            ".map_err(|s|${format(errorSymbol)}::unhandled(s))?"
        } else ""
        rust("output.build()$err")
    }

    private fun RustWriter.parseErrorVariants(
        operationShape: OperationShape,
        errorSymbol: RuntimeType,
        errorParsers: Map<ShapeId, Map<String, String>>
    ) {
        operationShape.errors.forEach { error ->
            val variantName = symbolProvider.toSymbol(model.expectShape(error)).name
            val parser = errorParsers[error.toShapeId()] ?: throw CodegenException("Parser must be defined")
            val shape = model.expectShape(error, StructureShape::class.java)
            withBlock(
                """${error.name.dq()} => #1T {
                meta: generic,
                kind: #1TKind::$variantName({""",
                "})" +
                    "},",
                errorSymbol
            ) {
                this.renderShapeParser(shape, shape, errorSymbol, parser)
            }
        }
        write("_ => #T::unhandled(generic)", errorSymbol)
    }

    /**
     * Generate a parser & a parsed value converter for each output member of `operationShape`
     *
     * Returns a map with key = memberName, value = parsedValue
     */
    private fun renderParseFunctions(
        shape: Shape,
        bindings: Map<String, HttpBinding>,
        errorSymbol: RuntimeType,
        httpBindingGenerator: ResponseBindingGenerator,
        implBlockWriter: RustWriter,
        bodyShape: StructureShape?
    ): Map<String, String> {
        return shape.members().map { member ->
            val binding = bindings[member.memberName] ?: throw CodegenException("Binding should be defined")
            member.memberName to when (binding.location) {
                HttpBinding.Location.HEADER -> {
                    val fnName = httpBindingGenerator.generateDeserializeHeaderFn(binding, implBlockWriter)
                    """
                        Self::$fnName(
                            response.headers()
                        ).map_err(|_|${implBlockWriter.format(errorSymbol)}::unhandled("Failed to parse ${member.memberName} from header `${binding.locationName}"))?
                        """
                }
                HttpBinding.Location.DOCUMENT -> {
                    check(bodyShape != null) {
                        "$bodyShape was null but a member specified document bindings. This is a bug."
                    }
                    // When there is a subset of fields present as the body of the response, we will create a variable
                    // named `parsed_body`. Copy the field from parsed_body into the builder

                    "parsed_body.${symbolProvider.toMemberName(member)}"
                }
                HttpBinding.Location.PAYLOAD -> {
                    val docShapeHandler: RustWriter.(String) -> Unit = { body ->
                        rustTemplate(
                            """
                            #{serde_json}::from_slice::<#{doc_json}::DeserDoc>($body).map(|d|d.0).map_err(#{error_symbol}::unhandled)
                        """,
                            "doc_json" to RuntimeType.DocJson,
                            "serde_json" to CargoDependency.SerdeJson.asType(),
                            "error_symbol" to errorSymbol
                        )
                    }
                    val structureShapeHandler: RustWriter.(String) -> Unit = { body ->
                        rust("#T($body).map_err(#T::unhandled)", RuntimeType.SerdeJson("from_slice"), errorSymbol)
                    }
                    val fnName = httpBindingGenerator.generateDeserializePayloadFn(
                        binding,
                        errorSymbol,
                        implBlockWriter,
                        docHandler = docShapeHandler,
                        structuredHandler = structureShapeHandler
                    )
                    "Self::$fnName(response.body().as_ref())?"
                }
                HttpBinding.Location.RESPONSE_CODE -> "Some(response.status().as_u16() as _)"
                else -> {
                    logger.warning("Unhandled response binding type: ${binding.location}")
                    "todo!()"
                }
            }
        }.toMap()
    }

    /**
     * Generate a parser & a parsed value converter for each output member of `operationShape`
     *
     * Returns a map with key = memberName, value = parsedValue
     */
    private fun renderParseFunctions(
        operationShape: OperationShape,
        httpBindingGenerator: ResponseBindingGenerator,
        implBlockWriter: RustWriter,
        bodyShape: StructureShape?
    ): Map<String, String> {
        val bindings = httpIndex.getResponseBindings(operationShape)
        val outputShape = operationShape.outputShape(model)
        val errorSymbol = operationShape.errorSymbol(symbolProvider)
        return outputShape.members().map { member ->
            val binding = bindings[member.memberName] ?: throw CodegenException("Binding should be defined")
            member.memberName to when (binding.location) {
                HttpBinding.Location.HEADER -> {
                    val fnName = httpBindingGenerator.generateDeserializeHeaderFn(binding, implBlockWriter)
                    """
                        Self::$fnName(
                            response.headers()
                        ).map_err(|_|${implBlockWriter.format(errorSymbol)}::unhandled("Failed to parse ${member.memberName} from header `${binding.locationName}"))?
                        """
                }
                HttpBinding.Location.DOCUMENT -> {
                    check(bodyShape != null) {
                        "$bodyShape was null but a member specified document bindings. This is a bug."
                    }
                    // When there is a subset of fields present as the body of the response, we will create a variable
                    // named `parsed_body`. Copy the field from parsed_body into the builder

                    "parsed_body.${symbolProvider.toMemberName(member)}"
                }
                HttpBinding.Location.PAYLOAD -> {
                    val docShapeHandler: RustWriter.(String) -> Unit = { body ->
                        rustTemplate(
                            """
                            #{serde_json}::from_slice::<#{doc_json}::DeserDoc>($body).map(|d|d.0).map_err(#{error_symbol}::unhandled)
                        """,
                            "doc_json" to RuntimeType.DocJson,
                            "serde_json" to CargoDependency.SerdeJson.asType(),
                            "error_symbol" to errorSymbol
                        )
                    }
                    val structureShapeHandler: RustWriter.(String) -> Unit = { body ->
                        rust("#T($body).map_err(#T::unhandled)", RuntimeType.SerdeJson("from_slice"), errorSymbol)
                    }
                    val fnName = httpBindingGenerator.generateDeserializePayloadFn(
                        binding,
                        errorSymbol,
                        implBlockWriter,
                        docHandler = docShapeHandler,
                        structuredHandler = structureShapeHandler
                    )
                    "Self::$fnName(response.body().as_ref())?"
                }
                HttpBinding.Location.RESPONSE_CODE -> "Some(response.status().as_u16() as _)"
                else -> {
                    logger.warning("Unhandled response binding type: ${binding.location}")
                    "todo!()"
                }
            }
        }.toMap()
    }

    private fun serializeViaSyntheticBody(
        implBlockWriter: RustWriter,
        inputBody: StructureShape
    ) {
        val bodySymbol = protocolConfig.symbolProvider.toSymbol(inputBody)
        implBlockWriter.rustBlock("fn body(&self) -> #T", bodySymbol) {
            rustBlock("#T", bodySymbol) {
                for (member in inputBody.members()) {
                    val name = protocolConfig.symbolProvider.toMemberName(member)
                    write("$name: &self.$name,")
                }
            }
        }
        bodyBuilderFun(implBlockWriter) {
            write("""#T(&self.body()).expect("serialization should succeed")""", RuntimeType.SerdeJson("to_vec"))
        }
    }

    override fun toBodyImpl(
        implBlockWriter: RustWriter,
        inputShape: StructureShape,
        inputBody: StructureShape?,
        operationShape: OperationShape
    ) {
        // If we created a synthetic input body, serialize that
        if (inputBody != null) {
            return serializeViaSyntheticBody(implBlockWriter, inputBody)
        }

        // Otherwise, we need to serialize via the HTTP payload trait
        val bindings = httpIndex.getRequestBindings(operationShape).toList()
        val payload: Pair<String, HttpBinding>? =
            bindings.firstOrNull { (_, binding) -> binding.location == HttpBinding.Location.PAYLOAD }
        val payloadSerde = payload?.let { (payloadMemberName, _) ->
            val member = inputShape.expectMember(payloadMemberName)
            val rustMemberName = "self.${symbolProvider.toMemberName(member)}"
            val targetShape = model.expectShape(member.target)
            writable {
                val payloadName = safeName()
                rust("let $payloadName = &$rustMemberName;")
                // If this targets a member & the member is None, return an empty vec
                if (symbolProvider.toSymbol(member).isOptional()) {
                    rust(
                        """
                        let $payloadName = match $payloadName.as_ref() {
                            Some(t) => t,
                            None => return vec![]
                        };"""
                    )
                }
                renderPayload(targetShape, payloadName)
            }
            // body is null, no payload set, so this is empty
        } ?: writable { rust("vec![]") }
        bodyBuilderFun(implBlockWriter) {
            payloadSerde(this)
        }
    }

    private fun RustWriter.renderPayload(
        targetShape: Shape,
        payloadName: String,
    ) {
        val serdeToVec = RuntimeType.SerdeJson("to_vec")
        when (targetShape) {
            // Write the raw string to the payload
            is StringShape ->
                if (targetShape.hasTrait(EnumTrait::class.java)) {
                    rust("$payloadName.as_str().into()")
                } else {
                    rust("""$payloadName.to_string().into()""")
                }
            is BlobShape ->
                // Write the raw blob to the payload
                rust("$payloadName.as_ref().into()")
            is StructureShape, is UnionShape ->
                // JSON serialize the structure or union targetted
                rust(
                    """#T(&$payloadName).expect("serialization should succeed")""",
                    serdeToVec
                )
            is DocumentShape ->
                rustTemplate(
                    """#{to_vec}(&#{doc_json}::SerDoc(&$payloadName)).expect("serialization should succeed")""",
                    "to_vec" to serdeToVec,
                    "doc_json" to RuntimeType.DocJson
                )
            else -> TODO("Unexpected payload target type")
        }
    }

    private val symbolProvider = protocolConfig.symbolProvider
    private val runtimeConfig = protocolConfig.runtimeConfig
    private val httpIndex = HttpBindingIndex.of(model)
    private val requestBuilder = RuntimeType.Http("request::Builder")

    override fun toHttpRequestImpl(
        implBlockWriter: RustWriter,
        operationShape: OperationShape,
        inputShape: StructureShape
    ) {
        val httpTrait = operationShape.expectTrait(HttpTrait::class.java)

        val httpBindingGenerator = RequestBindingGenerator(
            model,
            symbolProvider,
            runtimeConfig,
            implBlockWriter,
            operationShape,
            inputShape,
            httpTrait
        )
        val contentType =
            httpIndex.determineRequestContentType(operationShape, "application/json").orElse("application/json")
        httpBindingGenerator.renderUpdateHttpBuilder(implBlockWriter)
        httpBuilderFun(implBlockWriter) {
            rust(
                """
            let builder = #T::new();
            let builder = builder.header("Content-Type", ${contentType.dq()});
            self.update_http_builder(builder)
            """,
                requestBuilder
            )
        }
    }
}
