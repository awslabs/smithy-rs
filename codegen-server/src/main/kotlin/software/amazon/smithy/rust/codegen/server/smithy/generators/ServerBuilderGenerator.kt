/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.codegen.core.SymbolProvider
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.rust.codegen.client.rustlang.Attribute
import software.amazon.smithy.rust.codegen.client.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.client.rustlang.RustType
import software.amazon.smithy.rust.codegen.client.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.client.rustlang.Visibility
import software.amazon.smithy.rust.codegen.client.rustlang.conditionalBlock
import software.amazon.smithy.rust.codegen.client.rustlang.deprecatedShape
import software.amazon.smithy.rust.codegen.client.rustlang.docs
import software.amazon.smithy.rust.codegen.client.rustlang.documentShape
import software.amazon.smithy.rust.codegen.client.rustlang.implInto
import software.amazon.smithy.rust.codegen.client.rustlang.render
import software.amazon.smithy.rust.codegen.client.rustlang.rust
import software.amazon.smithy.rust.codegen.client.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.client.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.client.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.client.rustlang.stripOuter
import software.amazon.smithy.rust.codegen.client.rustlang.withBlock
import software.amazon.smithy.rust.codegen.client.rustlang.writable
import software.amazon.smithy.rust.codegen.client.smithy.PubCrateConstrainedShapeSymbolProvider
import software.amazon.smithy.rust.codegen.client.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.client.smithy.RustBoxTrait
import software.amazon.smithy.rust.codegen.client.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.client.smithy.ServerCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.expectRustMetadata
import software.amazon.smithy.rust.codegen.client.smithy.generators.StructureGenerator
import software.amazon.smithy.rust.codegen.client.smithy.generators.serverBuilderSymbol
import software.amazon.smithy.rust.codegen.client.smithy.hasConstraintTraitOrTargetHasConstraintTrait
import software.amazon.smithy.rust.codegen.client.smithy.isOptional
import software.amazon.smithy.rust.codegen.client.smithy.isRustBoxed
import software.amazon.smithy.rust.codegen.client.smithy.makeMaybeConstrained
import software.amazon.smithy.rust.codegen.client.smithy.makeOptional
import software.amazon.smithy.rust.codegen.client.smithy.makeRustBoxed
import software.amazon.smithy.rust.codegen.client.smithy.mapRustType
import software.amazon.smithy.rust.codegen.client.smithy.rustType
import software.amazon.smithy.rust.codegen.client.smithy.targetCanReachConstrainedShape
import software.amazon.smithy.rust.codegen.client.smithy.wouldHaveConstrainedWrapperTupleTypeWerePublicConstrainedTypesEnabled
import software.amazon.smithy.rust.codegen.core.smithy.traits.SyntheticInputTrait
import software.amazon.smithy.rust.codegen.core.util.hasTrait
import software.amazon.smithy.rust.codegen.core.util.isReachableFromOperationInput
import software.amazon.smithy.rust.codegen.core.util.letIf
import software.amazon.smithy.rust.codegen.core.util.toPascalCase
import software.amazon.smithy.rust.codegen.core.util.toSnakeCase
import software.amazon.smithy.rust.codegen.server.smithy.PubCrateConstraintViolationSymbolProvider
import software.amazon.smithy.rust.codegen.server.smithy.ServerRuntimeType

/**
 * Generates a builder for the Rust type associated with the [StructureShape].
 *
 * This generator is meant for use by the server project. Clients use the [BuilderGenerator] from the `codegen-client`
 * Gradle subproject instead.
 *
 * This builder is different in that it enforces [constraint traits] upon calling `.build()`. If any constraint
 * violations occur, the `build` method returns them.
 *
 * These are the main differences with the builders generated by the client's [BuilderGenerator]:
 *
 * - The design of this builder is simpler and closely follows what you get when using the [derive_builder] crate:
 *     * The builder may have one method per struct member named _exactly_ like the struct member and whose input type
 *       matches _exactly_ the struct's member type. This method is generated by [renderBuilderMemberFn].
 *     * The builder may have one _setter_ method (i.e. prefixed with `set_`) per struct member whose input type is the
 *       corresponding _unconstrained type_ for the member. This method is always `pub(crate)` and meant for use for
 *       server deserializers only.
 *     * There are no convenience methods to add items to vector and hash map struct members.
 * - The builder is not `PartialEq`. This is because the builder's members may or may not have been constrained (their
 *   types hold `MaybeConstrained`, and so it doesn't make sense to compare e.g. two builders holding the same data
 *   values, but one builder holds the member in the constrained variant while the other one holds it in the unconstrained
 *   variant.
 * - The builder always implement `TryFrom<Builder> for Structure` or `From<Builder> for Structure`, depending on whether
 *   the structure is constrained (and hence enforcing the constraints might yield an error) or not, respectively.
 *
 * The builder is `pub(crate)` when `publicConstrainedTypes` is `false`, since in this case the user is never exposed
 * to constrained types, and only the server's deserializers need to enforce constraint traits upon receiving a request.
 * The user is exposed to [ServerBuilderGeneratorWithoutPublicConstrainedTypes] in this case instead, which intentionally
 * _does not_ enforce constraints.
 *
 * [constraint traits]: https://awslabs.github.io/smithy/2.0/spec/constraint-traits.html
 * [derive_builder]: https://docs.rs/derive_builder/latest/derive_builder/index.html
 */
class ServerBuilderGenerator(
    codegenContext: ServerCodegenContext,
    private val shape: StructureShape,
) {
    private val takeInUnconstrainedTypes = shape.isReachableFromOperationInput()
    private val model = codegenContext.model
    private val publicConstrainedTypes = codegenContext.settings.codegenConfig.publicConstrainedTypes
    private val symbolProvider = codegenContext.symbolProvider
    private val constraintViolationSymbolProvider =
        with(codegenContext.constraintViolationSymbolProvider) {
            if (publicConstrainedTypes) {
                this
            } else {
                PubCrateConstraintViolationSymbolProvider(this)
            }
        }
    private val constrainedShapeSymbolProvider = codegenContext.constrainedShapeSymbolProvider
    private val pubCrateConstrainedShapeSymbolProvider = codegenContext.pubCrateConstrainedShapeSymbolProvider
    private val members: List<MemberShape> = shape.allMembers.values.toList()
    private val structureSymbol = symbolProvider.toSymbol(shape)
    private val builderSymbol = shape.serverBuilderSymbol(symbolProvider, !publicConstrainedTypes)
    private val moduleName = builderSymbol.namespace.split(builderSymbol.namespaceDelimiter).last()
    private val isBuilderFallible = StructureGenerator.serverHasFallibleBuilder(shape, model, symbolProvider, takeInUnconstrainedTypes)

    private val codegenScope = arrayOf(
        "RequestRejection" to ServerRuntimeType.RequestRejection(codegenContext.runtimeConfig),
        "Structure" to structureSymbol,
        "From" to RuntimeType.From,
        "TryFrom" to RuntimeType.TryFrom,
        "MaybeConstrained" to RuntimeType.MaybeConstrained(),
    )

    fun render(writer: RustWriter) {
        val visibility = if (publicConstrainedTypes) {
            Visibility.PUBLIC
        } else {
            Visibility.PUBCRATE
        }

        writer.docs("See #D.", structureSymbol)
        writer.withModule(moduleName, RustMetadata(visibility = visibility)) {
            renderBuilder(this)
        }
    }

    private fun renderBuilder(writer: RustWriter) {
        if (isBuilderFallible) {
            Attribute.Derives(setOf(RuntimeType.Debug, RuntimeType.PartialEq)).render(writer)
            writer.docs("Holds one variant for each of the ways the builder can fail.")
            Attribute.NonExhaustive.render(writer)
            val constraintViolationSymbolName = constraintViolationSymbolProvider.toSymbol(shape).name
            writer.rustBlock("pub enum $constraintViolationSymbolName") {
                constraintViolations().forEach {
                    renderConstraintViolation(
                        this,
                        it,
                        model,
                        constraintViolationSymbolProvider,
                        symbolProvider,
                        structureSymbol,
                    )
                }
            }

            renderImplDisplayConstraintViolation(writer)
            writer.rust("impl #T for ConstraintViolation { }", RuntimeType.StdError)

            // Only generate converter from `ConstraintViolation` into `RequestRejection` if the structure shape is
            // an operation input shape.
            if (shape.hasTrait<SyntheticInputTrait>()) {
                renderImplFromConstraintViolationForRequestRejection(writer)
            }

            if (takeInUnconstrainedTypes) {
                renderImplFromBuilderForMaybeConstrained(writer)
            }

            renderTryFromBuilderImpl(writer)
        } else {
            renderFromBuilderImpl(writer)
        }

        writer.docs("A builder for #D.", structureSymbol)
        // Matching derives to the main structure, - `PartialEq` (see class documentation for why), + `Default`
        // since we are a builder and everything is optional.
        val baseDerives = structureSymbol.expectRustMetadata().derives
        val derives = baseDerives.derives.intersect(setOf(RuntimeType.Debug, RuntimeType.Clone)) + RuntimeType.Default
        baseDerives.copy(derives = derives).render(writer)
        writer.rustBlock("pub struct Builder") {
            members.forEach { renderBuilderMember(this, it) }
        }

        writer.rustBlock("impl Builder") {
            for (member in members) {
                if (publicConstrainedTypes) {
                    renderBuilderMemberFn(this, member)
                }

                if (takeInUnconstrainedTypes) {
                    renderBuilderMemberSetterFn(this, member)
                }
            }
            renderBuildFn(this)
        }
    }

    // TODO This impl does not take into account the `sensitive` trait.
    private fun renderImplDisplayConstraintViolation(writer: RustWriter) {
        writer.rustBlock("impl #T for ConstraintViolation", RuntimeType.Display) {
            rustBlock("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result") {
                rustBlock("match self") {
                    constraintViolations().forEach {
                        val arm = if (it.hasInner()) {
                            "ConstraintViolation::${it.name()}(_)"
                        } else {
                            "ConstraintViolation::${it.name()}"
                        }
                        rust("""$arm => write!(f, "${constraintViolationMessage(it, symbolProvider, structureSymbol)}"),""")
                    }
                }
            }
        }
    }

    private fun renderImplFromConstraintViolationForRequestRejection(writer: RustWriter) {
        writer.rustTemplate(
            """
            impl #{From}<ConstraintViolation> for #{RequestRejection} {
                fn from(value: ConstraintViolation) -> Self {
                    Self::Build(value.into())
                }
            }
            """,
            *codegenScope,
        )
    }

    private fun renderImplFromBuilderForMaybeConstrained(writer: RustWriter) {
        writer.rustTemplate(
            """
            impl #{From}<Builder> for #{StructureMaybeConstrained} {
                fn from(builder: Builder) -> Self {
                    Self::Unconstrained(builder)
                }
            }
            """,
            *codegenScope,
            "StructureMaybeConstrained" to structureSymbol.makeMaybeConstrained(),
        )
    }

    private fun buildFnReturnType() = writable {
        if (isBuilderFallible) {
            rust("Result<#T, ConstraintViolation>", structureSymbol)
        } else {
            rust("#T", structureSymbol)
        }
    }

    private fun renderBuildFn(implBlockWriter: RustWriter) {
        implBlockWriter.docs("""Consumes the builder and constructs a #D.""", structureSymbol)
        if (isBuilderFallible) {
            implBlockWriter.docs(
                """
                The builder fails to construct a #D if a [`ConstraintViolation`] occurs.
                """,
                structureSymbol,
            )

            if (constraintViolations().size > 1) {
                implBlockWriter.docs("If the builder fails, it will return the _first_ encountered [`ConstraintViolation`].")
            }
        }
        implBlockWriter.rustTemplate(
            """
            pub fn build(self) -> #{ReturnType:W} {
                self.build_enforcing_all_constraints()
            }
            """,
            "ReturnType" to buildFnReturnType(),
        )
        renderBuildEnforcingAllConstraintsFn(implBlockWriter)
    }

    private fun renderBuildEnforcingAllConstraintsFn(implBlockWriter: RustWriter) {
        implBlockWriter.rustBlockTemplate("fn build_enforcing_all_constraints(self) -> #{ReturnType:W}", "ReturnType" to buildFnReturnType()) {
            conditionalBlock("Ok(", ")", conditional = isBuilderFallible) {
                coreBuilder(this)
            }
        }
    }

    fun renderConvenienceMethod(implBlock: RustWriter) {
        implBlock.docs("Creates a new builder-style object to manufacture #D.", structureSymbol)
        implBlock.rustBlock("pub fn builder() -> #T", builderSymbol) {
            write("#T::default()", builderSymbol)
        }
    }

    private fun renderBuilderMember(writer: RustWriter, member: MemberShape) {
        val memberSymbol = builderMemberSymbol(member)
        val memberName = constrainedShapeSymbolProvider.toMemberName(member)
        // Builder members are crate-public to enable using them directly in serializers/deserializers.
        // During XML deserialization, `builder.<field>.take` is used to append to lists and maps.
        writer.write("pub(crate) $memberName: #T,", memberSymbol)
    }

    /**
     * Render a `foo` method to set shape member `foo`. The caller must provide a value with the exact same type
     * as the shape member's type.
     *
     * This method is meant for use by the user; it is not used by the generated crate's (de)serializers.
     *
     * This method is only generated when `publicConstrainedTypes` is `true`. Otherwise, the user has at their disposal
     * the method from [ServerBuilderGeneratorWithoutPublicConstrainedTypes].
     */
    private fun renderBuilderMemberFn(
        writer: RustWriter,
        member: MemberShape,
    ) {
        check(publicConstrainedTypes)
        val symbol = symbolProvider.toSymbol(member)
        val memberName = symbolProvider.toMemberName(member)

        val hasBox = symbol.mapRustType { it.stripOuter<RustType.Option>() }.isRustBoxed()
        val wrapInMaybeConstrained = takeInUnconstrainedTypes && member.targetCanReachConstrainedShape(model, symbolProvider)

        writer.documentShape(member, model)
        writer.deprecatedShape(member)

        if (hasBox && wrapInMaybeConstrained) {
            // In the case of recursive shapes, the member might be boxed. If so, and the member is also constrained, the
            // implementation of this function needs to immediately unbox the value to wrap it in `MaybeConstrained`,
            // and then re-box. Clippy warns us that we could have just taken in an unboxed value to avoid this round-trip
            // to the heap. However, that will make the builder take in a value whose type does not exactly match the
            // shape member's type.
            // We don't want to introduce API asymmetry just for this particular case, so we disable the lint.
            Attribute.Custom("allow(clippy::boxed_local)").render(writer)
        }
        writer.rustBlock("pub fn $memberName(mut self, input: ${symbol.rustType().render()}) -> Self") {
            withBlock("self.$memberName = ", "; self") {
                conditionalBlock("Some(", ")", conditional = !symbol.isOptional()) {
                    val maybeConstrainedVariant =
                        "${symbol.makeMaybeConstrained().rustType().namespace}::MaybeConstrained::Constrained"

                    var varExpr = if (symbol.isOptional()) "v" else "input"
                    if (hasBox) varExpr = "*$varExpr"
                    if (!constrainedTypeHoldsFinalType(member)) varExpr = "($varExpr).into()"

                    if (wrapInMaybeConstrained) {
                        // TODO Add a protocol testing the branch (`symbol.isOptional() == false`, `hasBox == true`).
                        conditionalBlock("input.map(##[allow(clippy::redundant_closure)] |v| ", ")", conditional = symbol.isOptional()) {
                            conditionalBlock("Box::new(", ")", conditional = hasBox) {
                                rust("$maybeConstrainedVariant($varExpr)")
                            }
                        }
                    } else {
                        write("input")
                    }
                }
            }
        }
    }

    /**
     * Returns whether the constrained builder member type (the type on which the `Constrained` trait is implemented)
     * is the final type the user sees when receiving the built struct. This is true when the corresponding constrained
     * type is public and not `pub(crate)`, which happens when the target is a structure shape, a union shape, or is
     * directly constrained.
     *
     * An example where this returns false is when the member shape targets a list whose members are lists of structures
     * having at least one `required` member. In this case the member shape is transitively but not directly constrained,
     * so the generated constrained type is `pub(crate)` and needs converting into the final type the user will be
     * exposed to.
     *
     * See [PubCrateConstrainedShapeSymbolProvider] too.
     */
    private fun constrainedTypeHoldsFinalType(member: MemberShape): Boolean {
        val targetShape = model.expectShape(member.target)
        return targetShape is StructureShape ||
            targetShape is UnionShape ||
            member.hasConstraintTraitOrTargetHasConstraintTrait(model, symbolProvider)
    }

    /**
     * Render a `set_foo` method.
     * This method is able to take in unconstrained types for constrained shapes, like builders of structs in the case
     * of structure shapes.
     *
     * This method is only used by deserializers at the moment and is therefore `pub(crate)`.
     */
    private fun renderBuilderMemberSetterFn(
        writer: RustWriter,
        member: MemberShape,
    ) {
        val builderMemberSymbol = builderMemberSymbol(member)
        val inputType = builderMemberSymbol.rustType().stripOuter<RustType.Option>().implInto()
            .letIf(
                // TODO(https://github.com/awslabs/smithy-rs/issues/1302, https://github.com/awslabs/smithy/issues/1179):
                //  The only reason why this condition can't simply be `member.isOptional`
                //  is because non-`required` blob streaming members are interpreted as
                //  `required`, so we can't use `member.isOptional` here.
                symbolProvider.toSymbol(member).isOptional(),
            ) { "Option<$it>" }
        val memberName = symbolProvider.toMemberName(member)

        writer.documentShape(member, model)
        // Setter names will never hit a reserved word and therefore never need escaping.
        writer.rustBlock("pub(crate) fn set_${member.memberName.toSnakeCase()}(mut self, input: $inputType) -> Self") {
            rust(
                """
                self.$memberName = ${
                // TODO(https://github.com/awslabs/smithy-rs/issues/1302, https://github.com/awslabs/smithy/issues/1179): See above.
                if (symbolProvider.toSymbol(member).isOptional()) {
                    "input.map(|v| v.into())"
                } else {
                    "Some(input.into())"
                }
                };
                self
                """,
            )
        }
    }

    private fun constraintViolations() = members.flatMap { member ->
        listOfNotNull(
            builderMissingFieldConstraintViolationForMember(member, symbolProvider),
            builderConstraintViolationForMember(member),
        )
    }

    /**
     * Returns the builder failure associated with the `member` field if its target is constrained.
     */
    private fun builderConstraintViolationForMember(member: MemberShape) =
        if (takeInUnconstrainedTypes && member.targetCanReachConstrainedShape(model, symbolProvider)) {
            ConstraintViolation(member, ConstraintViolationKind.CONSTRAINED_SHAPE_FAILURE)
        } else {
            null
        }

    private fun renderTryFromBuilderImpl(writer: RustWriter) {
        writer.rustTemplate(
            """
            impl #{TryFrom}<Builder> for #{Structure} {
                type Error = ConstraintViolation;
                
                fn try_from(builder: Builder) -> Result<Self, Self::Error> {
                    builder.build()
                }
            }
            """,
            *codegenScope,
        )
    }

    private fun renderFromBuilderImpl(writer: RustWriter) {
        writer.rustTemplate(
            """
            impl #{From}<Builder> for #{Structure} {
                fn from(builder: Builder) -> Self {
                    builder.build()
                }
            }
            """,
            *codegenScope,
        )
    }

    /**
     * Returns the symbol for a builder's member.
     * All builder members are optional, but only some are `Option<T>`s where `T` needs to be constrained.
     */
    private fun builderMemberSymbol(member: MemberShape): Symbol =
        if (takeInUnconstrainedTypes && member.targetCanReachConstrainedShape(model, symbolProvider)) {
            val strippedOption = if (member.hasConstraintTraitOrTargetHasConstraintTrait(model, symbolProvider)) {
                constrainedShapeSymbolProvider.toSymbol(member)
            } else {
                pubCrateConstrainedShapeSymbolProvider.toSymbol(member)
            }
                // Strip the `Option` in case the member is not `required`.
                .mapRustType { it.stripOuter<RustType.Option>() }

            val hadBox = strippedOption.isRustBoxed()
            strippedOption
                // Strip the `Box` in case the member can reach itself recursively.
                .mapRustType { it.stripOuter<RustType.Box>() }
                // Wrap it in the Cow-like `constrained::MaybeConstrained` type, since we know the target member shape can
                // reach a constrained shape.
                .makeMaybeConstrained()
                // Box it in case the member can reach itself recursively.
                .letIf(hadBox) { it.makeRustBoxed() }
                // Ensure we always end up with an `Option`.
                .makeOptional()
        } else {
            constrainedShapeSymbolProvider.toSymbol(member).makeOptional()
        }

    /**
     * Writes the code to instantiate the struct the builder builds.
     *
     * Builder member types are either:
     *     1. `Option<MaybeConstrained<U>>`; or
     *     2. `Option<U>`.
     *
     * Where `U` is a constrained type.
     *
     * The structs they build have member types:
     *     a) `Option<T>`; or
     *     b) `T`.
     *
     * `U` is equal to `T` when:
     *     - the shape for `U` has a constraint trait and `publicConstrainedTypes` is `true`; or
     *     - the member shape is a structure or union shape.
     * Otherwise, `U` is always a `pub(crate)` tuple newtype holding `T`.
     *
     * For each member, this function first safely unwraps case 1. into 2., then converts `U` into `T` if necessary,
     * and then converts into b) if necessary.
     */
    private fun coreBuilder(writer: RustWriter) {
        writer.rustBlock("#T", structureSymbol) {
            for (member in members) {
                val memberName = symbolProvider.toMemberName(member)

                withBlock("$memberName: self.$memberName", ",") {
                    // Write the modifier(s).
                    builderConstraintViolationForMember(member)?.also { constraintViolation ->
                        val hasBox = builderMemberSymbol(member)
                            .mapRustType { it.stripOuter<RustType.Option>() }
                            .isRustBoxed()
                        if (hasBox) {
                            rustTemplate(
                                """
                                .map(|v| match *v {
                                    #{MaybeConstrained}::Constrained(x) => Ok(Box::new(x)),
                                    #{MaybeConstrained}::Unconstrained(x) => Ok(Box::new(x.try_into()?)),
                                })
                                .map(|res| 
                                    res${ if (constrainedTypeHoldsFinalType(member)) "" else ".map(|v| v.into())" }
                                       .map_err(|err| ConstraintViolation::${constraintViolation.name()}(Box::new(err)))
                                )
                                .transpose()?
                                """,
                                *codegenScope,
                            )
                        } else {
                            rustTemplate(
                                """
                                .map(|v| match v {
                                    #{MaybeConstrained}::Constrained(x) => Ok(x),
                                    #{MaybeConstrained}::Unconstrained(x) => x.try_into(),
                                })
                                .map(|res| 
                                    res${if (constrainedTypeHoldsFinalType(member)) "" else ".map(|v| v.into())"}
                                       .map_err(ConstraintViolation::${constraintViolation.name()})
                                )
                                .transpose()?
                                """,
                                *codegenScope,
                            )

                            // Constrained types are not public and this is a member shape that would have generated a
                            // public constrained type, were the setting to be enabled.
                            // We've just checked the constraints hold by going through the non-public
                            // constrained type, but the user wants to work with the unconstrained type, so we have to
                            // unwrap it.
                            if (!publicConstrainedTypes && member.wouldHaveConstrainedWrapperTupleTypeWerePublicConstrainedTypesEnabled(model)) {
                                rust(
                                    ".map(|v: #T| v.into())",
                                    constrainedShapeSymbolProvider.toSymbol(model.expectShape(member.target)),
                                )
                            }
                        }
                    }
                    builderMissingFieldConstraintViolationForMember(member, symbolProvider)?.also {
                        rust(".ok_or(ConstraintViolation::${it.name()})?")
                    }
                }
            }
        }
    }
}

/**
 * The kinds of constraint violations that can occur when building the builder.
 */
enum class ConstraintViolationKind {
    // A field is required but was not provided.
    MISSING_MEMBER,

    // An unconstrained type was provided for a field targeting a constrained shape, but it failed to convert into the constrained type.
    CONSTRAINED_SHAPE_FAILURE,
}

data class ConstraintViolation(val forMember: MemberShape, val kind: ConstraintViolationKind) {
    fun name() = when (kind) {
        ConstraintViolationKind.MISSING_MEMBER -> "Missing${forMember.memberName.toPascalCase()}"
        ConstraintViolationKind.CONSTRAINED_SHAPE_FAILURE -> forMember.memberName.toPascalCase()
    }

    /**
     * Whether the constraint violation is a Rust tuple struct with one element.
     */
    fun hasInner() = kind == ConstraintViolationKind.CONSTRAINED_SHAPE_FAILURE
}

/**
 * A message for a `ConstraintViolation` variant. This is used in both Rust documentation and the `Display` trait implementation.
 */
fun constraintViolationMessage(
    constraintViolation: ConstraintViolation,
    symbolProvider: RustSymbolProvider,
    structureSymbol: Symbol,
): String {
    val memberName = symbolProvider.toMemberName(constraintViolation.forMember)
    return when (constraintViolation.kind) {
        ConstraintViolationKind.MISSING_MEMBER -> "`$memberName` was not provided but it is required when building `${structureSymbol.name}`"
        // TODO Nest errors.
        ConstraintViolationKind.CONSTRAINED_SHAPE_FAILURE -> "constraint violation occurred building member `$memberName` when building `${structureSymbol.name}`"
    }
}

/**
 * Returns the builder failure associated with the [member] field if it is `required`.
 */
fun builderMissingFieldConstraintViolationForMember(member: MemberShape, symbolProvider: RustSymbolProvider) =
    // TODO(https://github.com/awslabs/smithy-rs/issues/1302, https://github.com/awslabs/smithy/issues/1179): See above.
    if (symbolProvider.toSymbol(member).isOptional()) {
        null
    } else {
        ConstraintViolation(member, ConstraintViolationKind.MISSING_MEMBER)
    }

fun renderConstraintViolation(
    writer: RustWriter,
    constraintViolation: ConstraintViolation,
    model: Model,
    constraintViolationSymbolProvider: SymbolProvider,
    symbolProvider: RustSymbolProvider,
    structureSymbol: Symbol,
) =
    when (constraintViolation.kind) {
        ConstraintViolationKind.MISSING_MEMBER -> {
            writer.docs(
                "${constraintViolationMessage(
                    constraintViolation,
                    symbolProvider,
                    structureSymbol,
                ).replaceFirstChar { it.uppercase() }}.",
            )
            writer.rust("${constraintViolation.name()},")
        }
        ConstraintViolationKind.CONSTRAINED_SHAPE_FAILURE -> {
            val targetShape = model.expectShape(constraintViolation.forMember.target)

            val constraintViolationSymbol =
                constraintViolationSymbolProvider.toSymbol(targetShape)
                    // If the corresponding structure's member is boxed, box this constraint violation symbol too.
                    .letIf(constraintViolation.forMember.hasTrait<RustBoxTrait>()) {
                        it.makeRustBoxed()
                    }

            // Note we cannot express the inner constraint violation as `<T as TryFrom<T>>::Error`, because `T` might
            // be `pub(crate)` and that would leak `T` in a public interface.
            writer.docs("${constraintViolationMessage(constraintViolation, symbolProvider, structureSymbol)}.")
            Attribute.DocHidden.render(writer)
            writer.rust("${constraintViolation.name()}(#T),", constraintViolationSymbol)
        }
    }
