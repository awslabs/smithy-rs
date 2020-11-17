/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.generators

import software.amazon.smithy.codegen.core.SymbolProvider
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ServiceShape
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.rust.codegen.lang.RustWriter
import software.amazon.smithy.rust.codegen.lang.rustBlock
import software.amazon.smithy.rust.codegen.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticInputTrait

/**
 * Configuration needed to generate the client for a given Service<->Protocol pair
 */
data class ProtocolConfig(
    val model: Model,
    val symbolProvider: SymbolProvider,
    val runtimeConfig: RuntimeConfig,
    val serviceShape: ServiceShape,
    val protocol: ShapeId
)

interface ProtocolGeneratorFactory<out T : HttpProtocolGenerator> {
    fun buildProtocolGenerator(protocolConfig: ProtocolConfig): T
    fun transformModel(model: Model): Model
    fun symbolProvider(model: Model, base: SymbolProvider): SymbolProvider = base
}

/**
 * Abstract class providing scaffolding for HTTP based protocols that must build an HTTP request (headers / URL) and
 * a body.
 */
abstract class HttpProtocolGenerator(
    protocolConfig: ProtocolConfig
) {
    private val symbolProvider = protocolConfig.symbolProvider
    private val model = protocolConfig.model
    fun renderOperation(writer: RustWriter, operationShape: OperationShape) {
        val inputShape = model.expectShape(operationShape.input.get(), StructureShape::class.java)
        writer.rustBlock("impl ${symbolProvider.toSymbol(inputShape).name}") {
            toHttpRequestImpl(this, operationShape, inputShape)
            val shapeId = inputShape.expectTrait(SyntheticInputTrait::class.java).body
            val body = shapeId?.let { model.expectShape(it, StructureShape::class.java) }
            toBodyImpl(this, inputShape, body)
        }
    }

    protected fun httpBuilderFun(implBlockWriter: RustWriter, f: RustWriter.() -> Unit) {
        implBlockWriter.rustBlock(
            "pub fn build_http_request(&self) -> \$T",
            RuntimeType.HttpRequestBuilder
        ) {
            f(this)
        }
    }

    open fun toBodyImpl(implBlockWriter: RustWriter, inputShape: StructureShape, inputBody: StructureShape?) {
        if (inputBody != null) {
            val bodySymbol = symbolProvider.toSymbol(inputBody)
            implBlockWriter.rustBlock("fn body(&self) -> \$T", bodySymbol) {
                rustBlock("\$T", bodySymbol) {
                    for (member in inputBody.members()) {
                        val name = symbolProvider.toMemberName(member)
                        write("$name: &self.$name,")
                    }
                }
            }
        }
        implBlockWriter.rustBlock("pub fn build_body(&self) -> Vec<u8>") {
            // TODO: use serde to serialize the body
            if (inputBody != null) {
                write("let _ = self.body();")
            }
            write("vec![]")
        }
    }

    /**
     * Add necessary methods to the impl block for the input shape.
     *
     * Your implementation MUST call `httpBuilderFun` to create the public method.
     */
    abstract fun toHttpRequestImpl(implBlockWriter: RustWriter, operationShape: OperationShape, inputShape: StructureShape)
}
