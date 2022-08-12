/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.util

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.BlobShape
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.TimestampShape
import software.amazon.smithy.rust.codegen.rustlang.Writable
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.smithy.rustType
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticInputTrait
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticOutputTrait

/*
 * Utility class used to force casting a non primitive type into one overriden by a new symbol provider,
 * by explicitly calling `from()` or into().
 *
 * For example we use this in the server Python implementation, where we override types like [Blob] and [DateTime]
 * with wrappers compatible with Python, without touching the original implementation coming from `aws-smithy-types`.
 */
class TypeConversionGenerator(private val model: Model, private val symbolProvider: RustSymbolProvider, private val runtimeConfig: RuntimeConfig) {
    private fun findOldSymbol(shape: Shape): Symbol {
        return when (shape) {
            is BlobShape -> {
                if (shape.isStreaming()) {
                    RuntimeType.ByteStream(runtimeConfig).toSymbol()
                } else {
                    RuntimeType.Blob(runtimeConfig).toSymbol()
                }
            }
            is TimestampShape -> RuntimeType.DateTime(runtimeConfig).toSymbol()
            else -> symbolProvider.toSymbol(shape)
        }
    }

    fun convertViaFrom(shape: Shape): Writable =
        writable {
            val oldSymbol = findOldSymbol(shape)
            val newSymbol = symbolProvider.toSymbol(shape)
            if (oldSymbol.rustType() != newSymbol.rustType()) {
                rust(".map($newSymbol::from)")
            }
        }

    fun convertViaInto(shape: Shape): Writable =
        writable {
            val oldSymbol = findOldSymbol(shape)
            val newSymbol = symbolProvider.toSymbol(shape)
            if (oldSymbol.rustType() != newSymbol.rustType()) {
                rust(".into()")
            }
        }

    fun Shape.isStreaming(): Boolean {
        if (this !is MemberShape) {
            return false
        }

        val container = model.expectShape(this.container)
        if (!(container.hasTrait<SyntheticOutputTrait>() || container.hasTrait<SyntheticInputTrait>())) {
            return false
        }

        return this.isStreaming(model)
    }
}
