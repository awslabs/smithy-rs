/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.ErrorTrait
import software.amazon.smithy.rust.codegen.core.rustlang.RustModule
import software.amazon.smithy.rust.codegen.core.rustlang.RustReservedWords
import software.amazon.smithy.rust.codegen.core.rustlang.Visibility
import software.amazon.smithy.rust.codegen.core.smithy.ModuleProvider
import software.amazon.smithy.rust.codegen.core.smithy.ModuleProviderContext
import software.amazon.smithy.rust.codegen.core.smithy.module
import software.amazon.smithy.rust.codegen.core.smithy.traits.SyntheticInputTrait
import software.amazon.smithy.rust.codegen.core.smithy.traits.SyntheticOutputTrait
import software.amazon.smithy.rust.codegen.core.util.hasTrait
import software.amazon.smithy.rust.codegen.core.util.toSnakeCase

object ServerRustModule {
    val root = RustModule.LibRs

    val Error = RustModule.public("error", documentation = "All error types that operations can return. Documentation on these types is copied from the model.")
    val Operation = RustModule.public("operation", documentation = "All operations that this crate can perform.")
    val Model = RustModule.public("model", documentation = "Data structures used by operation inputs/outputs. Documentation on these types is copied from the model.")
    val Input = RustModule.public("input", documentation = "Input structures for operations. Documentation on these types is copied from the model.")
    val Output = RustModule.public("output", documentation = "Output structures for operations. Documentation on these types is copied from the model.")
    val Types = RustModule.public("types", documentation = "Data primitives referenced by other data types.")

    val UnconstrainedModule =
        software.amazon.smithy.rust.codegen.core.smithy.UnconstrainedModule
    val ConstrainedModule =
        software.amazon.smithy.rust.codegen.core.smithy.ConstrainedModule
}

object ServerModuleProvider : ModuleProvider {
    override fun moduleForShape(context: ModuleProviderContext, shape: Shape): RustModule.LeafModule = when (shape) {
        is OperationShape -> ServerRustModule.Operation
        is StructureShape -> when {
            shape.hasTrait<ErrorTrait>() -> ServerRustModule.Error
            shape.hasTrait<SyntheticInputTrait>() -> ServerRustModule.Input
            shape.hasTrait<SyntheticOutputTrait>() -> ServerRustModule.Output
            else -> ServerRustModule.Model
        }
        else -> ServerRustModule.Model
    }

    override fun moduleForOperationError(
        context: ModuleProviderContext,
        operation: OperationShape,
    ): RustModule.LeafModule = ServerRustModule.Error

    override fun moduleForEventStreamError(
        context: ModuleProviderContext,
        eventStream: UnionShape,
    ): RustModule.LeafModule = ServerRustModule.Error

    override fun moduleForBuilder(context: ModuleProviderContext, shape: Shape, symbol: Symbol): RustModule.LeafModule {
        val pubCrate = !(context.settings as ServerRustSettings).codegenConfig.publicConstrainedTypes
        val builderNamespace = RustReservedWords.escapeIfNeeded(symbol.name.toSnakeCase()) +
            if (pubCrate) {
                "_internal"
            } else {
                ""
            }
        val visibility = when (pubCrate) {
            true -> Visibility.PUBCRATE
            false -> Visibility.PUBLIC
        }
        return RustModule.new(builderNamespace, visibility, parent = symbol.module(), inline = true)
    }
}
