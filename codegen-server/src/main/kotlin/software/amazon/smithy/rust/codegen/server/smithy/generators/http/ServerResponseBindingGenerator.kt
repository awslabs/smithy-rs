/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators.http

import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.generators.http.HttpBindingGenerator
import software.amazon.smithy.rust.codegen.core.smithy.generators.http.HttpMessageType
import software.amazon.smithy.rust.codegen.core.smithy.protocols.Protocol
import software.amazon.smithy.rust.codegen.server.smithy.ServerCodegenContext

class ServerResponseBindingGenerator(
    protocol: Protocol,
    codegenContext: ServerCodegenContext,
    operationShape: OperationShape,
) {
    private val httpBindingGenerator =
        HttpBindingGenerator(protocol, codegenContext, codegenContext.symbolProvider, operationShape)

    fun generateAddHeadersFn(shape: Shape): RuntimeType? =
        httpBindingGenerator.generateAddHeadersFn(shape, HttpMessageType.RESPONSE)
}
