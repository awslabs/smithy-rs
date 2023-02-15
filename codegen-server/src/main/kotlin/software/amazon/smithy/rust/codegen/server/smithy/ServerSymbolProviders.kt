/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy

import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.ServiceShape
import software.amazon.smithy.rust.codegen.core.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.core.smithy.RustSymbolProviderConfig

/**
 * Just a handy class to centralize initialization all the symbol providers required by the server code generators, to
 * make the init blocks of the codegen visitors ([ServerCodegenVisitor] and [PythonServerCodegenVisitor]), and the
 * unit test setup code, shorter and DRYer.
 */
class ServerSymbolProviders private constructor(
    val symbolProvider: RustSymbolProvider,
    val unconstrainedShapeSymbolProvider: UnconstrainedShapeSymbolProvider,
    val constrainedShapeSymbolProvider: RustSymbolProvider,
    val constraintViolationSymbolProvider: ConstraintViolationSymbolProvider,
    val pubCrateConstrainedShapeSymbolProvider: PubCrateConstrainedShapeSymbolProvider,
) {
    companion object {
        fun from(
            model: Model,
            service: ServiceShape,
            rustSymbolProviderConfig: RustSymbolProviderConfig,
            publicConstrainedTypes: Boolean,
            baseSymbolProviderFactory: (model: Model, service: ServiceShape, rustSymbolProviderConfig: RustSymbolProviderConfig, publicConstrainedTypes: Boolean) -> RustSymbolProvider,
        ): ServerSymbolProviders {
            val baseSymbolProvider = baseSymbolProviderFactory(model, service, rustSymbolProviderConfig, publicConstrainedTypes)
            return ServerSymbolProviders(
                symbolProvider = baseSymbolProvider,
                constrainedShapeSymbolProvider = baseSymbolProviderFactory(
                    model,
                    service,
                    rustSymbolProviderConfig,
                    true,
                ),
                unconstrainedShapeSymbolProvider = UnconstrainedShapeSymbolProvider(
                    baseSymbolProviderFactory(
                        model,
                        service,
                        rustSymbolProviderConfig,
                        false,
                    ),
                    publicConstrainedTypes, service,
                ),
                pubCrateConstrainedShapeSymbolProvider = PubCrateConstrainedShapeSymbolProvider(
                    baseSymbolProvider,
                    service,
                ),
                constraintViolationSymbolProvider = ConstraintViolationSymbolProvider(
                    baseSymbolProvider,
                    publicConstrainedTypes,
                    service,
                ),
            )
        }
    }
}
