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
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.HttpTrait
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.smithy.generators.HttpProtocolGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.HttpTraitBindingGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolConfig
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolGeneratorFactory
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolSupport
import software.amazon.smithy.rust.codegen.smithy.transformers.OperationNormalizer
import software.amazon.smithy.rust.codegen.util.dq

class AwsRestJsonFactory : ProtocolGeneratorFactory<AwsRestJsonGenerator> {
    override fun buildProtocolGenerator(
        protocolConfig: ProtocolConfig
    ): AwsRestJsonGenerator = AwsRestJsonGenerator(protocolConfig)

    /** Create a synthetic awsJsonInputBody if specified
     * A body is created iff no member of [input] is targeted with the `PAYLOAD` trait. If a member is targeted with
     * the payload trait, we don't need to create an input body.
     */
    private fun awsJsonInputBody(model: Model, operation: OperationShape, input: StructureShape?): StructureShape? {
        if (input == null) {
            return null
        }
        val bindingIndex = HttpBindingIndex.of(model)
        val bindings: MutableMap<String, HttpBinding> = bindingIndex.getRequestBindings(operation)
        if (bindings.values.map { it.location }.contains(HttpBinding.Location.PAYLOAD)) {
            return null
        }
        val currentMembers = input.members()
        val bodyMembers = currentMembers.filter { member ->
            bindings[member.memberName]?.location == HttpBinding.Location.DOCUMENT
        }
        return if (bodyMembers.isNotEmpty()) {
            input.toBuilder().members(bodyMembers).build()
        } else {
            null
        }
    }

    override fun transformModel(model: Model): Model {
        return OperationNormalizer(model).transformModel(
            inputBodyFactory = { op, input -> awsJsonInputBody(model, op, input) },
            outputBodyFactory = OperationNormalizer.NoBody
        )
    }

    override fun support(): ProtocolSupport {
        // TODO: Support body for RestJson
        return ProtocolSupport(
            requestBodySerialization = true,
            responseDeserialization = false,
            errorDeserialization = false
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

    private val model = protocolConfig.model
    override fun traitImplementations(operationWriter: RustWriter, operationShape: OperationShape) {
        // TODO: Implement parsing traits for AwsRestJson
    }

    override fun fromResponseImpl(implBlockWriter: RustWriter, operationShape: OperationShape) {
        fromResponseFun(implBlockWriter, operationShape) {
            // avoid non-usage warnings
            rust(
                """
                let _ = response;
                todo!()
            """
            )
        }
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
            val member =
                inputShape.getMember(payloadMemberName).orElseThrow { throw CodegenException("member should exist") }
            val rustMemberName = "self.${symbolProvider.toMemberName(member)}"
            val serdeToVec = RuntimeType.SerdeJson("to_vec")
            when (model.expectShape(member.target)) {
                // Write the raw string to the payload
                is StringShape -> writable { rust("$rustMemberName.into()") }
                is BlobShape -> writable {
                    // Write the raw blob to the payload
                    rust(
                        """ {
                        let slice = $rustMemberName.as_ref().map(|blob|blob.as_ref()).unwrap_or_default();
                        slice.into()
                        }
                    """
                    )
                }
                is StructureShape, is UnionShape -> writable {
                    // JSON serialize the structure or union targetted
                    rust(
                        """#T(&$rustMemberName).expect("serialization should succeed")""",
                        serdeToVec
                    )
                }
                is DocumentShape -> writable {
                    rustTemplate(
                        """#{to_vec}(&#{doc_json}::SerDoc(&$rustMemberName)).expect("serialization should succeed")""",
                        "to_vec" to serdeToVec,
                        "doc_json" to RuntimeType.DocJson
                    )
                }
                else -> TODO("Unexpected payload target type")
            }
            // body is null, no payload set, so this is empty
        } ?: writable { rust("vec![]") }
        bodyBuilderFun(implBlockWriter) {
            payloadSerde(this)
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

        val httpBindingGenerator = HttpTraitBindingGenerator(
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
