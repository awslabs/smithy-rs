/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.protocols.serialize

import org.junit.jupiter.api.Test
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.BlobShape
import software.amazon.smithy.model.shapes.CollectionShape
import software.amazon.smithy.model.shapes.ListShape
import software.amazon.smithy.model.shapes.MapShape
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.NumberShape
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.TimestampShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.ErrorTrait
import software.amazon.smithy.model.transform.ModelTransformer
import software.amazon.smithy.protocoltests.traits.AppliesTo
import software.amazon.smithy.rust.codegen.core.rustlang.Attribute
import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.core.rustlang.DependencyScope
import software.amazon.smithy.rust.codegen.core.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.core.smithy.SymbolMetadataProvider
import software.amazon.smithy.rust.codegen.core.smithy.expectRustMetadata
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.BrokenTest
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.FailingTest
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.ProtocolSupport
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.ProtocolTestGenerator
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.ServiceShapeId.RPC_V2_CBOR
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.TestCase
import software.amazon.smithy.rust.codegen.core.smithy.protocols.ProtocolFunctions
import software.amazon.smithy.rust.codegen.core.testutil.IntegrationTestParams
import software.amazon.smithy.rust.codegen.core.testutil.unitTest
import software.amazon.smithy.rust.codegen.core.util.UNREACHABLE
import software.amazon.smithy.rust.codegen.core.util.hasTrait
import software.amazon.smithy.rust.codegen.core.util.inputShape
import software.amazon.smithy.rust.codegen.core.util.toSnakeCase
import software.amazon.smithy.rust.codegen.server.smithy.ServerCodegenContext
import software.amazon.smithy.rust.codegen.server.smithy.customize.ServerCodegenDecorator
import software.amazon.smithy.rust.codegen.server.smithy.generators.ServerBuilderGenerator
import software.amazon.smithy.rust.codegen.server.smithy.generators.ServerInstantiator
import software.amazon.smithy.rust.codegen.server.smithy.generators.protocol.ServerProtocolTestGenerator
import software.amazon.smithy.rust.codegen.server.smithy.generators.protocol.ServerRpcV2CborProtocol
import software.amazon.smithy.rust.codegen.server.smithy.generators.serverBuilderSymbol
import software.amazon.smithy.rust.codegen.server.smithy.protocols.ServerRpcV2CborFactory
import software.amazon.smithy.rust.codegen.server.smithy.testutil.serverIntegrationTest
import java.util.function.Predicate
import java.util.logging.Logger

/**
 * This lives in `codegen-server` because we want to run a full integration test for convenience,
 * but there's really nothing server-specific here. We're just testing that the CBOR (de)serializers work like
 * the ones generated by `serde_cbor`. This is a good exhaustive litmus test for correctness, since `serde_cbor`
 * is battle-tested.
 */
internal class CborSerializerAndParserGeneratorSerdeRoundTripIntegrationTest {
    class DeriveSerdeSerializeDeserializeSymbolMetadataProvider(
        private val base: RustSymbolProvider,
    ) : SymbolMetadataProvider(base) {
        private val serdeDeserialize =
            CargoDependency.Serde.copy(scope = DependencyScope.Compile).toType().resolve("Deserialize")
        private val serdeSerialize =
            CargoDependency.Serde.copy(scope = DependencyScope.Compile).toType().resolve("Serialize")

        private fun addDeriveSerdeSerializeDeserialize(shape: Shape): RustMetadata {
            check(shape !is MemberShape)

            val baseMetadata = base.toSymbol(shape).expectRustMetadata()
            return baseMetadata.withDerives(serdeSerialize, serdeDeserialize)
        }

        override fun memberMeta(memberShape: MemberShape): RustMetadata {
            val baseMetadata = base.toSymbol(memberShape).expectRustMetadata()
            return baseMetadata.copy(
                additionalAttributes =
                    baseMetadata.additionalAttributes +
                        Attribute(
                            """serde(rename = "${memberShape.memberName}")""",
                            isDeriveHelper = true,
                        ),
            )
        }

        override fun structureMeta(structureShape: StructureShape) = addDeriveSerdeSerializeDeserialize(structureShape)

        override fun unionMeta(unionShape: UnionShape) = addDeriveSerdeSerializeDeserialize(unionShape)

        override fun enumMeta(stringShape: StringShape) = addDeriveSerdeSerializeDeserialize(stringShape)

        override fun listMeta(listShape: ListShape): RustMetadata = addDeriveSerdeSerializeDeserialize(listShape)

        override fun mapMeta(mapShape: MapShape): RustMetadata = addDeriveSerdeSerializeDeserialize(mapShape)

        override fun stringMeta(stringShape: StringShape): RustMetadata =
            addDeriveSerdeSerializeDeserialize(stringShape)

        override fun numberMeta(numberShape: NumberShape): RustMetadata =
            addDeriveSerdeSerializeDeserialize(numberShape)

        override fun blobMeta(blobShape: BlobShape): RustMetadata = addDeriveSerdeSerializeDeserialize(blobShape)
    }

    fun prepareRpcV2CborModel(): Model {
        var model = Model.assembler().discoverModels().assemble().result.get()

        // Filter out `timestamp` and `blob` shapes: those map to runtime types in `aws-smithy-types` on
        // which we can't `#[derive(serde::Deserialize)]`.
        // Note we can't use `ModelTransformer.removeShapes` because it will leave the model in an inconsistent state
        // when removing list/set shape member shapes.
        val removeTimestampAndBlobShapes: Predicate<Shape> =
            Predicate { shape ->
                when (shape) {
                    is MemberShape -> {
                        val targetShape = model.expectShape(shape.target)
                        targetShape is BlobShape || targetShape is TimestampShape
                    }
                    is BlobShape, is TimestampShape -> true
                    is CollectionShape -> {
                        val targetShape = model.expectShape(shape.member.target)
                        targetShape is BlobShape || targetShape is TimestampShape
                    }
                    else -> false
                }
            }

        fun removeShapesByShapeId(shapeIds: Set<ShapeId>): Predicate<Shape> {
            val predicate: Predicate<Shape> =
                Predicate { shape ->
                    when (shape) {
                        is MemberShape -> {
                            val targetShape = model.expectShape(shape.target)
                            shapeIds.contains(targetShape.id)
                        }
                        is CollectionShape -> {
                            val targetShape = model.expectShape(shape.member.target)
                            shapeIds.contains(targetShape.id)
                        }
                        else -> {
                            shapeIds.contains(shape.id)
                        }
                    }
                }
            return predicate
        }

        val modelTransformer = ModelTransformer.create()
        model =
            modelTransformer.removeShapesIf(
                modelTransformer.removeShapesIf(model, removeTimestampAndBlobShapes),
                // These enums do not serialize their variants using the Rust members' names.
                // We'd have to tack on `#[serde(rename = "name")]` using the proper name defined in the Smithy enum definition.
                // But we have no way of injecting that attribute on Rust enum variants in the code generator.
                // So we just remove these problematic shapes.
                removeShapesByShapeId(
                    setOf(
                        ShapeId.from("smithy.protocoltests.shared#FooEnum"),
                        ShapeId.from("smithy.protocoltests.rpcv2Cbor#TestEnum"),
                    ),
                ),
            )

        return model
    }

    @Test
    fun `serde_cbor round trip`() {
        val addDeriveSerdeSerializeDeserializeDecorator =
            object : ServerCodegenDecorator {
                override val name: String = "Add `#[derive(serde::Serialize, serde::Deserialize)]`"
                override val order: Byte = 0

                override fun symbolProvider(base: RustSymbolProvider): RustSymbolProvider =
                    DeriveSerdeSerializeDeserializeSymbolMetadataProvider(base)
            }

        // Don't generate protocol tests, because it'll attempt to pull out `params` for member shapes we'll remove
        // from the model.
        val noProtocolTestsDecorator =
            object : ServerCodegenDecorator {
                override val name: String = "Don't generate protocol tests"
                override val order: Byte = 0

                override fun protocolTestGenerator(
                    codegenContext: ServerCodegenContext,
                    baseGenerator: ProtocolTestGenerator,
                ): ProtocolTestGenerator {
                    val noOpProtocolTestsGenerator =
                        object : ProtocolTestGenerator() {
                            override val codegenContext: CodegenContext
                                get() = baseGenerator.codegenContext
                            override val protocolSupport: ProtocolSupport
                                get() = baseGenerator.protocolSupport
                            override val operationShape: OperationShape
                                get() = baseGenerator.operationShape
                            override val appliesTo: AppliesTo
                                get() = baseGenerator.appliesTo
                            override val logger: Logger
                                get() = Logger.getLogger(javaClass.name)
                            override val expectFail: Set<FailingTest>
                                get() = baseGenerator.expectFail
                            override val brokenTests: Set<BrokenTest>
                                get() = emptySet()
                            override val runOnly: Set<String>
                                get() = baseGenerator.runOnly
                            override val disabledTests: Set<String>
                                get() = baseGenerator.disabledTests

                            override fun RustWriter.renderAllTestCases(allTests: List<TestCase>) {
                                // No-op.
                            }
                        }
                    return noOpProtocolTestsGenerator
                }
            }

        val model = prepareRpcV2CborModel()
        val serviceShape = model.expectShape(ShapeId.from(RPC_V2_CBOR))
        serverIntegrationTest(
            model,
            additionalDecorators = listOf(addDeriveSerdeSerializeDeserializeDecorator, noProtocolTestsDecorator),
            params = IntegrationTestParams(service = serviceShape.id.toString()),
        ) { codegenContext, rustCrate ->
            // TODO(https://github.com/smithy-lang/smithy-rs/issues/1147): NaN != NaN. Ideally we when we address
            //  this issue, we'd re-use the structure shape comparison code that both client and server protocol test
            //  generators would use.
            val expectFail = setOf("RpcV2CborSupportsNaNFloatInputs", "RpcV2CborSupportsNaNFloatOutputs")

            val codegenScope =
                arrayOf(
                    "AssertEq" to RuntimeType.PrettyAssertions.resolve("assert_eq!"),
                    "SerdeCbor" to CargoDependency.SerdeCbor.toType(),
                )

            val instantiator = ServerInstantiator(codegenContext, ignoreMissingMembers = true)
            val rpcV2 = ServerRpcV2CborProtocol(codegenContext)

            for (operationShape in codegenContext.model.operationShapes) {
                val serverProtocolTestGenerator =
                    ServerProtocolTestGenerator(codegenContext, ServerRpcV2CborFactory().support(), operationShape)

                rustCrate.withModule(ProtocolFunctions.serDeModule) {
                    // The SDK can only serialize operation outputs, so we only ask for response tests.
                    val responseTests =
                        serverProtocolTestGenerator.responseTestCases()

                    for (test in responseTests) {
                        when (test) {
                            is TestCase.MalformedRequestTest -> UNREACHABLE("we did not ask for tests of this kind")
                            is TestCase.RequestTest -> UNREACHABLE("we did not ask for tests of this kind")
                            is TestCase.ResponseTest -> {
                                val targetShape = test.targetShape
                                val params = test.testCase.params

                                val serializeFn =
                                    if (targetShape.hasTrait<ErrorTrait>()) {
                                        rpcV2.structuredDataSerializer().serverErrorSerializer(targetShape.id)
                                    } else {
                                        rpcV2.structuredDataSerializer().operationOutputSerializer(operationShape)
                                    }

                                if (serializeFn == null) {
                                    // Skip if there's nothing to serialize.
                                    continue
                                }

                                if (expectFail.contains(test.id)) {
                                    writeWithNoFormatting("#[should_panic]")
                                }
                                unitTest("we_serialize_and_serde_cbor_deserializes_${test.id.toSnakeCase()}_${test.kind.toString().toSnakeCase()}") {
                                    rustTemplate(
                                        """
                                        let expected = #{InstantiateShape:W};
                                        let bytes = #{SerializeFn}(&expected)
                                            .expect("our generated CBOR serializer failed");
                                        let actual = #{SerdeCbor}::from_slice(&bytes)
                                           .expect("serde_cbor failed deserializing from bytes");
                                        #{AssertEq}(expected, actual);
                                        """,
                                        "InstantiateShape" to instantiator.generate(targetShape, params),
                                        "SerializeFn" to serializeFn,
                                        *codegenScope,
                                    )
                                }
                            }
                        }
                    }

                    // The SDK can only deserialize operation inputs, so we only ask for request tests.
                    val requestTests =
                        serverProtocolTestGenerator.requestTestCases()
                    val inputShape = operationShape.inputShape(codegenContext.model)
                    val err =
                        if (ServerBuilderGenerator.hasFallibleBuilder(
                                inputShape,
                                codegenContext.model,
                                codegenContext.symbolProvider,
                                takeInUnconstrainedTypes = true,
                            )
                        ) {
                            """.expect("builder failed to build")"""
                        } else {
                            ""
                        }

                    for (test in requestTests) {
                        when (test) {
                            is TestCase.MalformedRequestTest -> UNREACHABLE("we did not ask for tests of this kind")
                            is TestCase.ResponseTest -> UNREACHABLE("we did not ask for tests of this kind")
                            is TestCase.RequestTest -> {
                                val targetShape = operationShape.inputShape(codegenContext.model)
                                val params = test.testCase.params

                                val deserializeFn =
                                    rpcV2.structuredDataParser().serverInputParser(operationShape)
                                        ?: // Skip if there's nothing to serialize.
                                        continue

                                if (expectFail.contains(test.id)) {
                                    writeWithNoFormatting("#[should_panic]")
                                }
                                unitTest("serde_cbor_serializes_and_we_deserialize_${test.id.toSnakeCase()}_${test.kind.toString().toSnakeCase()}") {
                                    rustTemplate(
                                        """
                                        let expected = #{InstantiateShape:W};
                                        let bytes: Vec<u8> = #{SerdeCbor}::to_vec(&expected)
                                            .expect("serde_cbor failed serializing to `Vec<u8>`");
                                        let input = #{InputBuilder}::default();
                                        let input = #{DeserializeFn}(&bytes, input)
                                           .expect("our generated CBOR deserializer failed");
                                        let actual = input.build()$err;
                                        #{AssertEq}(expected, actual);
                                        """,
                                        "InstantiateShape" to instantiator.generate(targetShape, params),
                                        "DeserializeFn" to deserializeFn,
                                        "InputBuilder" to inputShape.serverBuilderSymbol(codegenContext),
                                        *codegenScope,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
