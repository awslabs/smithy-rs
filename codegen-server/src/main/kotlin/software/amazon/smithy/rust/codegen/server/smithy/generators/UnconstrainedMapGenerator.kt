/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.model.shapes.MapShape
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.rust.codegen.core.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Visibility
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.smithy.canReachConstrainedShape
import software.amazon.smithy.rust.codegen.core.smithy.isDirectlyConstrained
import software.amazon.smithy.rust.codegen.core.smithy.makeMaybeConstrained
import software.amazon.smithy.rust.codegen.server.smithy.PubCrateConstraintViolationSymbolProvider
import software.amazon.smithy.rust.codegen.server.smithy.ServerCodegenContext

/**
 * Generates a Rust type for a constrained map shape that is able to hold values for the corresponding
 * _unconstrained_ shape. This type is a [RustType.Opaque] wrapper tuple newtype holding a `HashMap`. Upon request parsing,
 * server deserializers use this type to store the incoming values without enforcing the modeled constraints. Only after
 * the full request has been parsed are constraints enforced, via the `impl TryFrom<UnconstrainedSymbol> for
 * ConstrainedSymbol`.
 *
 * This type is never exposed to the user; it is always `pub(crate)`. Only the deserializers use it.
 *
 * Consult [UnconstrainedShapeSymbolProvider] for more details and for an example.
 */
class UnconstrainedMapGenerator(
    val codegenContext: ServerCodegenContext,
    private val unconstrainedModuleWriter: RustWriter,
    val shape: MapShape,
) {
    private val model = codegenContext.model
    private val symbolProvider = codegenContext.symbolProvider
    private val unconstrainedShapeSymbolProvider = codegenContext.unconstrainedShapeSymbolProvider
    private val pubCrateConstrainedShapeSymbolProvider = codegenContext.pubCrateConstrainedShapeSymbolProvider
    private val symbol = unconstrainedShapeSymbolProvider.toSymbol(shape)
    private val name = symbol.name
    private val publicConstrainedTypes = codegenContext.settings.codegenConfig.publicConstrainedTypes
    private val constraintViolationSymbolProvider =
        with(codegenContext.constraintViolationSymbolProvider) {
            if (publicConstrainedTypes) {
                this
            } else {
                PubCrateConstraintViolationSymbolProvider(this)
            }
        }
    private val constraintViolationSymbol = constraintViolationSymbolProvider.toSymbol(shape)
    private val constrainedShapeSymbolProvider = codegenContext.constrainedShapeSymbolProvider
    private val constrainedSymbol = if (shape.isDirectlyConstrained(symbolProvider)) {
        constrainedShapeSymbolProvider.toSymbol(shape)
    } else {
        pubCrateConstrainedShapeSymbolProvider.toSymbol(shape)
    }
    private val keyShape = model.expectShape(shape.key.target, StringShape::class.java)
    private val valueShape = model.expectShape(shape.value.target)

    fun render() {
        check(shape.canReachConstrainedShape(model, symbolProvider))

        val module = symbol.namespace.split(symbol.namespaceDelimiter).last()
        val keySymbol = unconstrainedShapeSymbolProvider.toSymbol(keyShape)
        val valueSymbol = unconstrainedShapeSymbolProvider.toSymbol(valueShape)

        unconstrainedModuleWriter.withModule(module, RustMetadata(visibility = Visibility.PUBCRATE)) {
            rustTemplate(
                """
                ##[derive(Debug, Clone)]
                pub(crate) struct $name(pub(crate) std::collections::HashMap<#{KeySymbol}, #{ValueSymbol}>);
                
                impl From<$name> for #{MaybeConstrained} {
                    fn from(value: $name) -> Self {
                        Self::Unconstrained(value)
                    }
                }
                
                """,
                "KeySymbol" to keySymbol,
                "ValueSymbol" to valueSymbol,
                "MaybeConstrained" to constrainedSymbol.makeMaybeConstrained(),
            )

            renderTryFromUnconstrainedForConstrained(this)
        }
    }

    private fun renderTryFromUnconstrainedForConstrained(writer: RustWriter) {
        writer.rustBlock("impl std::convert::TryFrom<$name> for #{T}", constrainedSymbol) {
            rust("type Error = #T;", constraintViolationSymbol)

            rustBlock("fn try_from(value: $name) -> Result<Self, Self::Error>") {
                if (isKeyConstrained(keyShape, symbolProvider) || isValueConstrained(valueShape, model, symbolProvider)) {
                    val resolveToNonPublicConstrainedValueType =
                        isValueConstrained(valueShape, model, symbolProvider) &&
                            !valueShape.isDirectlyConstrained(symbolProvider) &&
                            !valueShape.isStructureShape
                    val constrainedValueSymbol = if (resolveToNonPublicConstrainedValueType) {
                        pubCrateConstrainedShapeSymbolProvider.toSymbol(valueShape)
                    } else {
                        constrainedShapeSymbolProvider.toSymbol(valueShape)
                    }

                    // TODO Refactor to use `return` so that we don't have to clone `k`.
                    rustTemplate(
                        """
                        let res: Result<std::collections::HashMap<#{ConstrainedKeySymbol}, #{ConstrainedValueSymbol}>, Self::Error> = value.0
                            .into_iter()
                            .map(|(k, v)| {
                                ${if (isKeyConstrained(keyShape, symbolProvider)) "let k: #{ConstrainedKeySymbol} = k.try_into().map_err(Self::Error::Key)?;" else ""}
                                ${if (isValueConstrained(valueShape, model, symbolProvider)) "let v: #{ConstrainedValueSymbol} = v.try_into().map_err(|inner_violation| Self::Error::Value(k.clone(), inner_violation))?;" else ""}
                                Ok((k, v))
                            })
                            .collect();
                        let hm = res?;
                        """,
                        "ConstrainedKeySymbol" to constrainedShapeSymbolProvider.toSymbol(keyShape),
                        "ConstrainedValueSymbol" to constrainedValueSymbol,
                    )

                    val constrainedValueTypeIsNotFinalType =
                        resolveToNonPublicConstrainedValueType && shape.isDirectlyConstrained(symbolProvider)
                    if (constrainedValueTypeIsNotFinalType) {
                        // The map is constrained. Its value shape reaches a constrained shape, but the value shape itself
                        // is not directly constrained. The value shape must be an aggregate shape. But it is not a
                        // structure shape. So it must be a collection or map shape. In this case the type for the value
                        // shape that implements the `Constrained` trait _does not_ coincide with the regular type the user
                        // is exposed to. The former will be the `pub(crate)` wrapper tuple type created by a
                        // `Constrained*Generator`, whereas the latter will be an stdlib container type. Both types are
                        // isomorphic though, and we can convert between them using `From`, so that's what we do here.
                        //
                        // As a concrete example of this particular case, consider the model:
                        //
                        // ```smithy
                        // @length(min: 1)
                        // map Map {
                        //    key: String,
                        //    value: List,
                        // }
                        //
                        // list List {
                        //     member: NiceString
                        // }
                        //
                        // @length(min: 1, max: 69)
                        // string NiceString
                        // ```
                        rustTemplate(
                            """
                            let hm: std::collections::HashMap<#{KeySymbol}, #{ValueSymbol}> = 
                                hm.into_iter().map(|(k, v)| (k, v.into())).collect();
                            """,
                            "KeySymbol" to symbolProvider.toSymbol(keyShape),
                            "ValueSymbol" to symbolProvider.toSymbol(valueShape),
                        )
                    }
                } else {
                    rust("let hm = value.0;")
                }

                if (shape.isDirectlyConstrained(symbolProvider)) {
                    rust("Self::try_from(hm)")
                } else {
                    rust("Ok(Self(hm))")
                }
            }
        }
    }
}
