/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.model.node.Node
import software.amazon.smithy.model.node.ObjectNode
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.withBlock
import software.amazon.smithy.rust.codegen.core.rustlang.withBlockTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.core.smithy.generators.BuilderInstantiator
import software.amazon.smithy.rust.codegen.core.smithy.generators.Instantiator
import software.amazon.smithy.rust.codegen.core.smithy.generators.InstantiatorCustomization
import software.amazon.smithy.rust.codegen.core.smithy.generators.InstantiatorSection
import software.amazon.smithy.rust.codegen.core.smithy.isOptional
import software.amazon.smithy.rust.codegen.core.smithy.protocols.parse.ReturnSymbolToParse
import software.amazon.smithy.rust.codegen.server.smithy.ServerCodegenContext
import software.amazon.smithy.rust.codegen.server.smithy.isDirectlyConstrained
import software.amazon.smithy.rust.codegen.server.smithy.traits.isReachableFromOperationInput

class ServerAfterInstantiatingValueConstrainItIfNecessary(val codegenContext: CodegenContext) :
    InstantiatorCustomization() {

    override fun section(section: InstantiatorSection): Writable = when (section) {
        is InstantiatorSection.AfterInstantiatingValue -> writable {
            if (section.shape.isDirectlyConstrained(codegenContext.symbolProvider)) {
                rust(""".try_into().expect("this is only used in tests")""")
            }
        }
    }
}

class ServerBuilderKindBehavior(val codegenContext: CodegenContext) : Instantiator.BuilderKindBehavior {
    override fun hasFallibleBuilder(shape: StructureShape): Boolean {
        // Only operation input builders take in unconstrained types.
        val takesInUnconstrainedTypes = shape.isReachableFromOperationInput()

        val publicConstrainedTypes = if (codegenContext is ServerCodegenContext) {
            codegenContext.settings.codegenConfig.publicConstrainedTypes
        } else {
            true
        }

        return if (publicConstrainedTypes) {
            ServerBuilderGenerator.hasFallibleBuilder(
                shape,
                codegenContext.model,
                codegenContext.symbolProvider,
                takesInUnconstrainedTypes,
            )
        } else {
            ServerBuilderGeneratorWithoutPublicConstrainedTypes.hasFallibleBuilder(
                shape,
                codegenContext.symbolProvider,
            )
        }
    }

    override fun setterName(memberShape: MemberShape): String = codegenContext.symbolProvider.toMemberName(memberShape)

    override fun doesSetterTakeInOption(memberShape: MemberShape): Boolean =
        codegenContext.symbolProvider.toSymbol(memberShape).isOptional()
}

class ServerInstantiator(private val codegenContext: CodegenContext) : Instantiator(
    codegenContext.symbolProvider,
    codegenContext.model,
    codegenContext.runtimeConfig,
    ServerBuilderKindBehavior(codegenContext),
    defaultsForRequiredFields = true,
    customizations = listOf(ServerAfterInstantiatingValueConstrainItIfNecessary(codegenContext)),
) {
    fun generate(
        shape: StructureShape,
        values: Map<MemberShape, Writable>,
        data: ObjectNode = Node.objectNode(),
        ctx: Ctx = Ctx(),
    ) = writable {
        render(this, shape, values, data, ctx)
    }

    fun render(
        writer: RustWriter,
        shape: StructureShape,
        values: Map<MemberShape, Writable>,
        data: ObjectNode,
        ctx: Ctx = Ctx(),
    ) {
        val symbolProvider = codegenContext.symbolProvider
        writer.withBlock("{", "}") {
            for (member in shape.members()) {
                val dataHasValue = data.getMember(member.memberName).isPresent
                val hasValue = values.containsKey(member) || dataHasValue
                check(member.isOptional || hasValue)
                val value = if (member.isOptional && !hasValue) {
                    writable("None")
                } else {
                    writable {
                        values[member]
                            ?.let { it(this) }
                            ?: super.renderMember(
                                this,
                                member,
                                data.expectMember(member.memberName),
                                ctx,
                            )
                    }
                }
                val name = symbolProvider.toMemberName(member)
                writer.rustTemplate(
                    "let $name = #{value};",
                    "value" to value,
                )
            }
            writer.withBlockTemplate("#{T} {", "}", "T" to symbolProvider.toSymbol(shape)) {
                for (member in shape.members()) {
                    val name = symbolProvider.toMemberName(member)
                    writer.rustTemplate("$name,")
                }
            }
        }
    }
}

class ServerBuilderInstantiator(
    private val symbolProvider: RustSymbolProvider,
    private val symbolParseFn: (Shape) -> ReturnSymbolToParse,
) : BuilderInstantiator {
    override fun setField(builder: String, value: Writable, field: MemberShape): Writable {
        // Server builders have the ability to have non-optional fields. When one of these fields is used,
        // we need to use `if let(...)` to only set the field when it is present.
        return if (!symbolProvider.toSymbol(field).isOptional()) {
            writable {
                val n = safeName()
                rustTemplate(
                    """
                    if let Some($n) = #{value} {
                        #{setter}
                    }
                    """,
                    "value" to value, "setter" to setFieldWithSetter(builder, writable(n), field),
                )
            }
        } else {
            setFieldWithSetter(builder, value, field)
        }
    }

    override fun finalizeBuilder(builder: String, shape: StructureShape, mapErr: Writable?): Writable = writable {
        val returnSymbolToParse = symbolParseFn(shape)
        if (returnSymbolToParse.isUnconstrained) {
            rust(builder)
        } else {
            rust("$builder.build()")
        }
    }
}
