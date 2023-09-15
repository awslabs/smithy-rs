/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
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

/**
 * Server enums do not have an `Unknown` variant like client enums do, so constructing an enum from
 * a string is a fallible operation (hence `try_from`). It's ok to panic here if construction fails,
 * since this is only used in protocol tests.
 */
private fun enumFromStringFn(enumSymbol: Symbol, data: String): Writable = writable {
    rust(
        """#T::try_from($data).expect("this is only used in tests")""",
        enumSymbol,
    )
}

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

fun serverInstantiator(codegenContext: CodegenContext) =
    Instantiator(
        codegenContext.symbolProvider,
        codegenContext.model,
        codegenContext.runtimeConfig,
        ServerBuilderKindBehavior(codegenContext),
        ::enumFromStringFn,
        defaultsForRequiredFields = true,
        customizations = listOf(ServerAfterInstantiatingValueConstrainItIfNecessary(codegenContext)),
    )

class ServerBuilderInstantiator(
    private val symbolProvider: RustSymbolProvider,
    private val symbolParseFn: (Shape) -> ReturnSymbolToParse,
) :
    BuilderInstantiator {
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
                    "value" to value, "setter" to setFieldBase(builder, writable(n), field),
                )
            }
        } else {
            setFieldBase(builder, value, field)
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
