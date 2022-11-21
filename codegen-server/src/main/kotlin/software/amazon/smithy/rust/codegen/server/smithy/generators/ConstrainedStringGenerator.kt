/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.traits.AbstractTrait
import software.amazon.smithy.model.traits.LengthTrait
import software.amazon.smithy.model.traits.PatternTrait
import software.amazon.smithy.rust.codegen.core.rustlang.Attribute
import software.amazon.smithy.rust.codegen.core.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.core.rustlang.RustModule
import software.amazon.smithy.rust.codegen.core.rustlang.RustType
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Visibility
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.docs
import software.amazon.smithy.rust.codegen.core.rustlang.documentShape
import software.amazon.smithy.rust.codegen.core.rustlang.render
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.core.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.makeMaybeConstrained
import software.amazon.smithy.rust.codegen.core.util.orNull
import software.amazon.smithy.rust.codegen.core.util.redactIfNecessary
import software.amazon.smithy.rust.codegen.server.smithy.PubCrateConstraintViolationSymbolProvider
import software.amazon.smithy.rust.codegen.server.smithy.ServerCargoDependency
import software.amazon.smithy.rust.codegen.server.smithy.ServerCodegenContext
import software.amazon.smithy.rust.codegen.server.smithy.traits.isReachableFromOperationInput
import software.amazon.smithy.rust.codegen.server.smithy.validationErrorMessage

private val supportedStringConstraintTraits = listOf(LengthTrait::class.java, PatternTrait::class.java)

/**
 * [ConstrainedStringGenerator] generates a wrapper tuple newtype holding a constrained `String`.
 * This type can be built from unconstrained values, yielding a `ConstraintViolation` when the input does not satisfy
 * the constraints.
 */
class ConstrainedStringGenerator(
    val codegenContext: ServerCodegenContext,
    val writer: RustWriter,
    val shape: StringShape,
) {
    val model = codegenContext.model
    val constrainedShapeSymbolProvider = codegenContext.constrainedShapeSymbolProvider
    val publicConstrainedTypes = codegenContext.settings.codegenConfig.publicConstrainedTypes
    private val constraintViolationSymbolProvider =
        with(codegenContext.constraintViolationSymbolProvider) {
            if (publicConstrainedTypes) {
                this
            } else {
                PubCrateConstraintViolationSymbolProvider(this)
            }
        }
    private val constraintsInfo: List<TraitInfo> =
        supportedStringConstraintTraits
            .mapNotNull { shape.getTrait(it).orNull() }
            .mapNotNull(TraitInfo::fromTrait)

    private fun renderTryFrom(inner: String, name: String, constraintViolation: Symbol) {
        writer.rustBlock("impl $name") {
            for (traitInfo in constraintsInfo) {
                rustTemplate("#ValidationFunction:W", "ValidationFunction" to traitInfo.validationFunctionDefinition(constraintViolation))
            }
        }

        writer.rustBlockTemplate("impl #{TryFrom}<$inner> for $name", "TryFrom" to RuntimeType.TryFrom) {
            rustTemplate("type Error = #{ConstraintViolation};", "ConstraintViolation" to constraintViolation)
            rustBlock(
                """
                /// ${rustDocsTryFromMethod(name, inner)}
                fn try_from(value: $inner) -> Result<Self, Self::Error>
                """,
            ) {
                for (traitInfo in constraintsInfo) {
                    rustTemplate("#TryFromCheck:W", "TryFromCheck" to traitInfo.tryFromCheck)
                }
                rust("Ok(Self(value))")
            }
        }
    }

    fun render() {
        val symbol = constrainedShapeSymbolProvider.toSymbol(shape)
        val name = symbol.name
        val inner = RustType.String.render()
        val constraintViolation = constraintViolationSymbolProvider.toSymbol(shape)

        val constrainedTypeVisibility = if (publicConstrainedTypes) {
            Visibility.PUBLIC
        } else {
            Visibility.PUBCRATE
        }
        val constrainedTypeMetadata = RustMetadata(
            Attribute.Derives(setOf(RuntimeType.Debug, RuntimeType.Clone, RuntimeType.PartialEq, RuntimeType.Eq, RuntimeType.Hash)),
            visibility = constrainedTypeVisibility,
        )

        // Note that we're using the linear time check `chars().count()` instead of `len()` on the input value, since the
        // Smithy specification says the `length` trait counts the number of Unicode code points when applied to string shapes.
        // https://awslabs.github.io/smithy/1.0/spec/core/constraint-traits.html#length-trait
        writer.documentShape(shape, model, note = rustDocsNote(name))
        constrainedTypeMetadata.render(writer)
        writer.rust("struct $name(pub(crate) $inner);")
        if (constrainedTypeVisibility == Visibility.PUBCRATE) {
            Attribute.AllowUnused.render(writer)
        }
        writer.rust(
            """
            impl $name {
                /// Extracts a string slice containing the entire underlying `String`.
                pub fn as_str(&self) -> &str {
                    &self.0
                }

                /// ${rustDocsInnerMethod(inner)}
                pub fn inner(&self) -> &$inner {
                    &self.0
                }

                /// ${rustDocsIntoInnerMethod(inner)}
                pub fn into_inner(self) -> $inner {
                    self.0
                }
            }""",
        )

        renderTryFrom(inner, name, constraintViolation)

        writer.rustTemplate(
            """
            impl #{ConstrainedTrait} for $name  {
                type Unconstrained = $inner;
            }

            impl #{From}<$inner> for #{MaybeConstrained} {
                fn from(value: $inner) -> Self {
                    Self::Unconstrained(value)
                }
            }

            impl #{Display} for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                   ${shape.redactIfNecessary(model, "self.0")}.fmt(f)
                }
            }


            impl #{From}<$name> for $inner {
                fn from(value: $name) -> Self {
                    value.into_inner()
                }
            }
            """,
            "ConstrainedTrait" to RuntimeType.ConstrainedTrait(),
            "ConstraintViolation" to constraintViolation,
            "MaybeConstrained" to symbol.makeMaybeConstrained(),
            "Display" to RuntimeType.Display,
            "From" to RuntimeType.From,
        )

        val constraintViolationModuleName = constraintViolation.namespace.split(constraintViolation.namespaceDelimiter).last()
        writer.withModule(RustModule(constraintViolationModuleName, RustMetadata(visibility = constrainedTypeVisibility))) {
            renderConstraintViolationEnum(this, shape, constraintViolation)
        }
    }

    private fun renderConstraintViolationEnum(writer: RustWriter, shape: StringShape, constraintViolation: Symbol) {
        writer.rustBlock(
            """
            ##[derive(Debug, PartialEq)]
            pub enum ${constraintViolation.name}
            """,
        ) {
            for (traitInfo in constraintsInfo) {
                rustTemplate(
                    "#ConstraintViolationVariant",
                    "ConstraintViolationVariant:W" to traitInfo.constraintViolationVariant,
                )
            }
        }

        if (shape.isReachableFromOperationInput()) {
            writer.rustBlock("impl ${constraintViolation.name}") {
                rustBlockTemplate(
                    "pub(crate) fn as_validation_exception_field(self, path: #{String}) -> crate::model::ValidationExceptionField",
                    "String" to RuntimeType.String,
                ) {
                    rustBlock("match self") {
                        for (traitInfo in constraintsInfo) {
                            rustTemplate("#ValidationExceptionField:W", "ValidationExceptionField" to traitInfo.asValidationExceptionField)
                        }
                    }
                }
            }
        }
    }
}

/**
 * Information needed to render a constraint trait as Rust code.
 */
private data class TraitInfo(
    val tryFromCheck: Writable,
    val constraintViolationVariant: Writable,
    val asValidationExceptionField: Writable,
    val validationFunctionDefinition: (constraintViolation: Symbol) -> Writable,
) {
    companion object {
        fun fromTrait(trait: AbstractTrait): TraitInfo? {
            return when (trait) {
                is LengthTrait -> {
                    this.fromLengthTrait(trait)
                }

                is PatternTrait -> {
                    this.fromPatternTrait(trait)
                }

                else -> {
                    null
                }
            }
        }

        private fun fromLengthTrait(lengthTrait: LengthTrait): TraitInfo {
            return TraitInfo(
                { rust("Self::check_length(&value)?;") },
                {
                    docs("Error when a string doesn't satisfy its `@length` requirements.")
                    rust("Length(usize),")
                },
                {
                    rust(
                        """
                        Self::Length(length) => crate::model::ValidationExceptionField {
                            message: format!("${lengthTrait.validationErrorMessage()}", length, &path),
                            path,
                        },
                        """,
                    )
                },
                { constraintViolation -> { renderLengthValidation(this, lengthTrait, constraintViolation) } },
            )
        }

        private fun fromPatternTrait(patternTrait: PatternTrait): TraitInfo {
            return TraitInfo(
                { rust("Self::check_pattern(&value)?;") },
                {
                    docs("Error when a string doesn't satisfy its `@pattern`.")
                    rust("Pattern(&'static str),")
                },
                {
                    rust(
                        """
                        Self::Pattern(pattern) => crate::model::ValidationExceptionField {
                            message: format!("${patternTrait.validationErrorMessage()}", &path, pattern),
                            path
                        },
                        """,
                    )
                },
                { constraintViolation -> { renderPatternValidation(this, patternTrait, constraintViolation) } },
            )
        }
    }
}

/**
 * Renders a `check_length` function to validate the string matches the
 * required length indicated by the `@length` trait.
 */
private fun renderLengthValidation(writer: RustWriter, lengthTrait: LengthTrait, constraintViolation: Symbol) {
    val condition = if (lengthTrait.min.isPresent && lengthTrait.max.isPresent) {
        "(${lengthTrait.min.get()}..=${lengthTrait.max.get()}).contains(&length)"
    } else if (lengthTrait.min.isPresent) {
        "${lengthTrait.min.get()} <= length"
    } else {
        "length <= ${lengthTrait.max.get()}"
    }
    writer.rust(
        """
        fn check_length(string: &str) -> Result<(), $constraintViolation> {
            let length = string.chars().count();

            if $condition {
                Ok(())
            } else {
                Err($constraintViolation::Length(length))
            }
        }
        """,
    )
}

/**
 * Renders a `check_pattern` function to validate the string matches the
 * supplied regex in the `@pattern` trait.
 */
private fun renderPatternValidation(writer: RustWriter, patternTrait: PatternTrait, constraintViolation: Symbol) {
    val pattern = patternTrait.pattern

    writer.rustTemplate(
        """
        fn check_pattern(string: &str) -> Result<(), $constraintViolation> {
            static REGEX : #{OnceCell}::sync::Lazy<#{Regex}::Regex> = #{OnceCell}::sync::Lazy::new(|| #{Regex}::Regex::new(r#"$pattern"#).unwrap());

            if REGEX.is_match(string) {
                Ok(())
            } else {
                Err($constraintViolation::Pattern(r#"$pattern"#))
            }
        }
        """,
        "Regex" to ServerCargoDependency.Regex.toType(),
        "OnceCell" to ServerCargoDependency.OnceCell.toType(),
    )
}
