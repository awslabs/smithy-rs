/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.model.knowledge.TopDownIndex
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.rust.codegen.core.rustlang.Attribute
import software.amazon.smithy.rust.codegen.core.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.core.rustlang.RustModule
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Visibility
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.smithy.RustCrate
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.ProtocolSupport
import software.amazon.smithy.rust.codegen.core.util.toPascalCase
import software.amazon.smithy.rust.codegen.server.smithy.generators.protocol.ServerProtocol
import software.amazon.smithy.rust.codegen.server.smithy.generators.protocol.ServerProtocolGenerator
import software.amazon.smithy.rust.codegen.server.smithy.generators.protocol.ServerProtocolTestGenerator

/**
 * ServerServiceGenerator
 *
 * Service generator is the main code generation entry point for Smithy services. Individual structures and unions are
 * generated in codegen visitor, but this class handles all protocol-specific code generation (i.e. operations).
 */
open class ServerServiceGenerator(
    private val rustCrate: RustCrate,
    private val protocolGenerator: ServerProtocolGenerator,
    private val protocolSupport: ProtocolSupport,
    val protocol: ServerProtocol,
    private val codegenContext: CodegenContext,
) {
    private val index = TopDownIndex.of(codegenContext.model)
    protected val operations = index.getContainedOperations(codegenContext.serviceShape).sortedBy { it.id }
    private val serviceName = codegenContext.serviceShape.id.name.toString()

    /**
     * Render Service Specific code. Code will end up in different files via [useShapeWriter]. See `SymbolVisitor.kt`
     * which assigns a symbol location to each shape.
     */
    fun render() {
        rustCrate.lib {
            rust("##[doc(inline)]")
            rust("pub use crate::service::$serviceName;")
        }

        rustCrate.withModule(RustModule.operation(Visibility.PRIVATE)) {
            ServerProtocolTestGenerator(codegenContext, protocolSupport, protocolGenerator).render(this)
        }

        for (operation in operations) {
            if (operation.errors.isNotEmpty()) {
                rustCrate.withModule(RustModule.Error) {
                    renderCombinedErrors(this, operation)
                }
            }
        }
        rustCrate.withModule(RustModule.private("operation_handler", "Operation handlers definition and implementation.")) {
            renderOperationHandler(this, operations)
        }
        rustCrate.withModule(
            RustModule(
                "operation_registry",
                RustMetadata(
                    visibility = Visibility.PUBLIC,
                    additionalAttributes = listOf(
                        Attribute.Deprecated("0.52.0", "This module exports the deprecated `OperationRegistry`. Use the service builder exported from your root crate."),
                    ),
                ),
                """
                Contains the [`operation_registry::OperationRegistry`], a place where
                you can register your service's operation implementations.

                ## Deprecation

                This service builder is deprecated - use [`${codegenContext.serviceShape.id.name.toPascalCase()}::builder_with_plugins`] or [`${codegenContext.serviceShape.id.name.toPascalCase()}::builder_without_plugins`] instead.
                """,
            ),
        ) {
            renderOperationRegistry(this, operations)
        }

        rustCrate.withModule(
            RustModule.public("operation_shape"),
        ) {
            ServerOperationShapeGenerator(operations, codegenContext).render(this)
        }

        rustCrate.withModule(
            RustModule.public("service"),
        ) {
            ServerServiceGeneratorV2(
                codegenContext,
                protocol,
            ).render(this)
        }

        renderExtras(operations)
    }

    // Render any extra section needed by subclasses of `ServerServiceGenerator`.
    open fun renderExtras(operations: List<OperationShape>) { }

    // Render combined errors.
    open fun renderCombinedErrors(writer: RustWriter, operation: OperationShape) {
        /* Subclasses can override */
    }

    // Render operations handler.
    open fun renderOperationHandler(writer: RustWriter, operations: List<OperationShape>) {
        ServerOperationHandlerGenerator(codegenContext, protocol, operations).render(writer)
    }

    // Render operations registry.
    private fun renderOperationRegistry(writer: RustWriter, operations: List<OperationShape>) {
        ServerOperationRegistryGenerator(codegenContext, protocol, operations).render(writer)
    }
}
