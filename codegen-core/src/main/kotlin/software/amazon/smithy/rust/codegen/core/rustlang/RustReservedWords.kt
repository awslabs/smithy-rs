/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.core.rustlang

import software.amazon.smithy.codegen.core.ReservedWordSymbolProvider
import software.amazon.smithy.codegen.core.ReservedWords
import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.shapes.EnumShape
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.EnumTrait
import software.amazon.smithy.rust.codegen.core.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.core.smithy.WrappingSymbolProvider
import software.amazon.smithy.rust.codegen.core.smithy.generators.UnionGenerator
import software.amazon.smithy.rust.codegen.core.smithy.renamedFrom
import software.amazon.smithy.rust.codegen.core.util.hasTrait
import software.amazon.smithy.rust.codegen.core.util.letIf

class RustReservedWordSymbolProvider(private val base: RustSymbolProvider) : WrappingSymbolProvider(base) {
    private val internal =
        ReservedWordSymbolProvider.builder().symbolProvider(base).memberReservedWords(RustReservedWords).build()

    override fun toMemberName(shape: MemberShape): String {
        val baseName = super.toMemberName(shape)
        val reservedWordReplacedName = internal.toMemberName(shape)
        val container = model.expectShape(shape.container)
        return when {
            container is StructureShape -> when (baseName) {
                "build" -> "build_value"
                "builder" -> "builder_value"
                "default" -> "default_value"
                "send" -> "send_value"
                // To avoid conflicts with the `make_operation` and `presigned` functions on generated inputs
                "make_operation" -> "make_operation_value"
                "presigned" -> "presigned_value"
                "customize" -> "customize_value"
                // To avoid conflicts with the error metadata `meta` field
                "meta" -> "meta_value"
                else -> reservedWordReplacedName
            }

            container is UnionShape -> when (baseName) {
                // Unions contain an `Unknown` variant. This exists to support parsing data returned from the server
                // that represent union variants that have been added since this SDK was generated.
                UnionGenerator.UnknownVariantName -> "${UnionGenerator.UnknownVariantName}Value"
                "${UnionGenerator.UnknownVariantName}Value" -> "${UnionGenerator.UnknownVariantName}Value_"
                // Self cannot be used as a raw identifier, so we can't use the normal escaping strategy
                // https://internals.rust-lang.org/t/raw-identifiers-dont-work-for-all-identifiers/9094/4
                "Self" -> "SelfValue"
                // Real models won't end in `_` so it's safe to stop here
                "SelfValue" -> "SelfValue_"
                else -> reservedWordReplacedName
            }

            container is EnumShape || container.hasTrait<EnumTrait>() -> when (baseName) {
                // Self cannot be used as a raw identifier, so we can't use the normal escaping strategy
                // https://internals.rust-lang.org/t/raw-identifiers-dont-work-for-all-identifiers/9094/4
                "Self" -> "SelfValue"
                // Real models won't end in `_` so it's safe to stop here
                "SelfValue" -> "SelfValue_"
                // Unknown is used as the name of the variant containing unexpected values
                "Unknown" -> "UnknownValue"
                // Real models won't end in `_` so it's safe to stop here
                "UnknownValue" -> "UnknownValue_"
                else -> reservedWordReplacedName
            }

            else -> error("unexpected container: $container")
        }
    }

    /**
     * Convert shape to a Symbol
     *
     * If this symbol provider renamed the symbol, a `renamedFrom` field will be set on the symbol, enabling
     * code generators to generate special docs.
     */
    override fun toSymbol(shape: Shape): Symbol {
        // Sanity check that the symbol provider stack is set up correctly
        check(super.toSymbol(shape).renamedFrom() == null) {
            "RustReservedWordSymbolProvider should only run once"
        }

        var renamedSymbol = internal.toSymbol(shape)
        return when (shape) {
            is MemberShape -> {
                val container = model.expectShape(shape.container)
                val containerIsEnum = container is EnumShape || container.hasTrait<EnumTrait>()
                if (container !is StructureShape && container !is UnionShape && !containerIsEnum) {
                    return base.toSymbol(shape)
                }
                val previousName = base.toMemberName(shape)
                val escapedName = this.toMemberName(shape)
                // if the names don't match and it isn't a simple escaping with `r#`, record a rename
                renamedSymbol.toBuilder().name(escapedName)
                    .letIf(escapedName != previousName && !escapedName.contains("r#")) {
                        it.renamedFrom(previousName)
                    }.build()
            }

            else -> base.toSymbol(shape)
        }
    }
}

object RustReservedWords : ReservedWords {
    private val RustKeywords = setOf(
        "as",
        "break",
        "const",
        "continue",
        "crate",
        "else",
        "enum",
        "extern",
        "false",
        "fn",
        "for",
        "if",
        "impl",
        "in",
        "let",
        "loop",
        "match",
        "mod",
        "move",
        "mut",
        "pub",
        "ref",
        "return",
        "self",
        "Self",
        "static",
        "struct",
        "super",
        "trait",
        "true",
        "type",
        "unsafe",
        "use",
        "where",
        "while",

        "async",
        "await",
        "dyn",

        "abstract",
        "become",
        "box",
        "do",
        "final",
        "macro",
        "override",
        "priv",
        "typeof",
        "unsized",
        "virtual",
        "yield",
        "try",
    )

    private val cantBeRaw = setOf("self", "crate", "super")

    override fun escape(word: String): String = when {
        cantBeRaw.contains(word) -> "${word}_"
        else -> "r##$word"
    }

    fun escapeIfNeeded(word: String): String = when (isReserved(word)) {
        true -> escape(word)
        else -> word
    }

    override fun isReserved(word: String): Boolean = RustKeywords.contains(word)
}
