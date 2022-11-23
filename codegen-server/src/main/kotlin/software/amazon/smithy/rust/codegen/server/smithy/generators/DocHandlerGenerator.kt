/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.smithy.CodegenTarget
import software.amazon.smithy.rust.codegen.core.smithy.Errors
import software.amazon.smithy.rust.codegen.core.smithy.Inputs
import software.amazon.smithy.rust.codegen.core.smithy.Outputs
import software.amazon.smithy.rust.codegen.core.smithy.generators.error.errorSymbol
import software.amazon.smithy.rust.codegen.core.util.inputShape
import software.amazon.smithy.rust.codegen.core.util.outputShape

/**
Generates a stub for use within documentation.
 */
class DocHandlerGenerator(private val operation: OperationShape, private val commentToken: String = "//", private val handlerName: String, codegenContext: CodegenContext) {
    private val model = codegenContext.model
    private val symbolProvider = codegenContext.symbolProvider
    private val crateName = codegenContext.moduleUseName()

    /**
     * Returns the function signature for an operation handler implementation. Used in the documentation.
     */
    fun docSignature(): Writable {
        val inputSymbol = symbolProvider.toSymbol(operation.inputShape(model))
        val outputSymbol = symbolProvider.toSymbol(operation.outputShape(model))
        val errorSymbol = operation.errorSymbol(model, symbolProvider, CodegenTarget.SERVER)

        val outputT = if (operation.errors.isEmpty()) {
            outputSymbol.name
        } else {
            "Result<${outputSymbol.name}, ${errorSymbol.name}>"
        }

        return writable {
            if (operation.errors.isNotEmpty()) {
                rust("$commentToken ## use $crateName::${Errors.namespace}::${errorSymbol.name};")
            }
            rust(
                """
                $commentToken ## use $crateName::${Inputs.namespace}::${inputSymbol.name};
                $commentToken ## use $crateName::${Outputs.namespace}::${outputSymbol.name};
                $commentToken async fn $handlerName(input: ${inputSymbol.name}) -> $outputT {
                $commentToken     todo!()
                $commentToken }
                """.trimIndent(),
            )
        }
    }

    fun render(writer: RustWriter) {
        docSignature()(writer)
    }
}
