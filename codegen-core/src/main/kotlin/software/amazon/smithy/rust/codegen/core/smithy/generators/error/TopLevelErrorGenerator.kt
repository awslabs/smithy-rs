/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.core.smithy.generators.error

import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.rust.codegen.core.rustlang.Attribute
import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.core.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.core.rustlang.RustModule
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Visibility
import software.amazon.smithy.rust.codegen.core.rustlang.asType
import software.amazon.smithy.rust.codegen.core.rustlang.deprecatedShape
import software.amazon.smithy.rust.codegen.core.rustlang.docs
import software.amazon.smithy.rust.codegen.core.rustlang.documentShape
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.core.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.smithy.CodegenTarget
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.RustCrate
import software.amazon.smithy.rust.codegen.core.smithy.transformers.allErrors
import software.amazon.smithy.rust.codegen.core.smithy.transformers.eventStreamErrors

/**
 * Each service defines its own "top-level" error combining all possible errors that a service can emit.
 *
 * Every service error is convertible into this top level error, which enables (if desired) authoring a single error handling
 * path. Eg:
 * ```rust
 * // dynamodb/src/lib.rs
 * enum Error {
 *   ListTablesError(ListTablesError),
 *   ValidationError(ValidationError),
 *   ...,
 *   // It also includes cases from SdkError
 * }
 * ```
 */
class TopLevelErrorGenerator(private val codegenContext: CodegenContext, private val operations: List<OperationShape>) {
    private val symbolProvider = codegenContext.symbolProvider
    private val model = codegenContext.model

    private val allErrors = operations.flatMap { it.allErrors(model) }.map { it.id }.distinctBy { it.getName(codegenContext.serviceShape) }
        .map { codegenContext.model.expectShape(it, StructureShape::class.java) }
        .sortedBy { it.id.getName(codegenContext.serviceShape) }

    private val sdkError = CargoDependency.SmithyHttp(codegenContext.runtimeConfig).asType().member("result::SdkError")
    fun render(crate: RustCrate) {
        crate.withModule(RustModule.private("error_meta")) {
            renderDefinition()
            renderImplDisplay()
            // Every operation error can be converted into service::Error
            operations.forEach { operationShape ->
                // operation errors
                renderImplFrom(operationShape.errorSymbol(model, symbolProvider, codegenContext.target), operationShape.errors)
            }
            // event stream errors
            operations.map { it.eventStreamErrors(codegenContext.model) }
                .flatMap { it.entries }
                .associate { it.key to it.value }
                .forEach { (unionShape, errors) ->
                    renderImplFrom(
                        unionShape.eventStreamErrorSymbol(
                            model,
                            symbolProvider,
                            codegenContext.target,
                        ),
                        errors.map { it.id },
                    )
                }
            rust("impl #T for Error {}", RuntimeType.StdError)
        }
        crate.lib { rust("pub use error_meta::Error;") }
    }

    private fun RustWriter.renderImplDisplay() {
        rustBlock("impl #T for Error", RuntimeType.Display) {
            rustBlock("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result") {
                rustBlock("match self") {
                    allErrors.forEach {
                        rust("Error::${symbolProvider.toSymbol(it).name}(inner) => inner.fmt(f),")
                    }
                    rust("Error::Unhandled(inner) => inner.fmt(f)")
                }
            }
        }
    }

    private fun RustWriter.renderImplFrom(errorSymbol: RuntimeType, errors: List<ShapeId>) {
        if (errors.isNotEmpty() || CodegenTarget.CLIENT == codegenContext.target) {
            val operationErrors = errors.map { model.expectShape(it) }
            rustBlock(
                "impl<R> From<#T<#T, R>> for Error where R: Send + Sync + std::fmt::Debug + 'static",
                sdkError,
                errorSymbol,
            ) {
                rustBlockTemplate(
                    "fn from(err: #{SdkError}<#{OpError}, R>) -> Self",
                    "SdkError" to sdkError,
                    "OpError" to errorSymbol,
                ) {
                    rustBlock("match err") {
                        rust("#T::ServiceError(context) => Self::from(context.into_err()),", sdkError)
                        rust("_ => Error::Unhandled(#T::new(err.into())),", unhandledError())
                    }
                }
            }

            rustBlock("impl From<#T> for Error", errorSymbol) {
                rustBlock("fn from(err: #T) -> Self", errorSymbol) {
                    rustBlock("match err.kind") {
                        operationErrors.forEach { errorShape ->
                            val errSymbol = symbolProvider.toSymbol(errorShape)
                            rust(
                                "#TKind::${errSymbol.name}(inner) => Error::${errSymbol.name}(inner),",
                                errorSymbol,
                            )
                        }
                        rustTemplate(
                            "#{errorSymbol}Kind::Unhandled(inner) => Error::Unhandled(#{unhandled}::new(inner.into())),",
                            "errorSymbol" to errorSymbol,
                            "unhandled" to unhandledError(),
                        )
                    }
                }
            }
        }
    }

    private fun RustWriter.renderDefinition() {
        rust("/// All possible error types for this service.")
        RustMetadata(
            additionalAttributes = listOf(Attribute.NonExhaustive),
            visibility = Visibility.PUBLIC,
        ).withDerives(RuntimeType.Debug).render(this)
        rustBlock("enum Error") {
            allErrors.forEach { error ->
                documentShape(error, model)
                deprecatedShape(error)
                val sym = symbolProvider.toSymbol(error)
                rust("${sym.name}(#T),", sym)
            }
            docs(UNHANDLED_ERROR_DOCS)
            rust("Unhandled(#T)", unhandledError())
        }
    }
}
