/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.generators

import software.amazon.smithy.codegen.core.writer.CodegenWriterDelegator
import software.amazon.smithy.model.knowledge.TopDownIndex
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.rust.codegen.lang.RustWriter
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticInputTrait
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticOutputTrait
import software.amazon.smithy.rust.codegen.util.inputShape

class ServiceGenerator(
    private val writers: CodegenWriterDelegator<RustWriter>,
    private val protocolGenerator: HttpProtocolGenerator,
    private val protocolSupport: ProtocolSupport,
    private val config: ProtocolConfig
) {
    private val index = TopDownIndex.of(config.model)

    fun render() {
        val operations = index.getContainedOperations(config.serviceShape).sortedBy { it.id }
        operations.map { operation ->
            writers.useShapeWriter(operation) { operationWriter ->
                writers.useShapeWriter(operation.inputShape(config.model)) { inputWriter ->
                    protocolGenerator.renderOperation(operationWriter, inputWriter, operation)
                    HttpProtocolTestGenerator(config, protocolSupport, operation, operationWriter).render()
                }
            }
        }
        renderBodies()
    }

    private fun renderBodies() {
        val operations = index.getContainedOperations(config.serviceShape)
        val inputBodies = operations.map { config.model.expectShape(it.input.get()) }.map {
            it.expectTrait(SyntheticInputTrait::class.java)
        }.mapNotNull { // mapNotNull is flatMap but for null `map { it }.filter { it != null }`
            it.body
        }.map { // Lookup the Body structure by its id
            config.model.expectShape(it, StructureShape::class.java)
        }
        val outputBodies = operations.map { config.model.expectShape(it.output.get()) }.map {
            it.expectTrait(SyntheticOutputTrait::class.java)
        }.mapNotNull { // mapNotNull is flatMap but for null `map { it }.filter { it != null }`
            it.body
        }.map { // Lookup the Body structure by its id
            config.model.expectShape(it, StructureShape::class.java)
        }
        (inputBodies + outputBodies).map { body ->
            // The body symbol controls its location, usually in the serializer module
            writers.useShapeWriter(body) { writer ->
                with(config) {
                    // Generate a body via the structure generator
                    StructureGenerator(model, symbolProvider, writer, body).render()
                }
            }
        }
    }
}
