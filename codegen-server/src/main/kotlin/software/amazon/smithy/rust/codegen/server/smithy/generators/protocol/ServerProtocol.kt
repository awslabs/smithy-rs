/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators.protocol

import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ResourceShape
import software.amazon.smithy.model.shapes.ServiceShape
import software.amazon.smithy.rust.codegen.rustlang.Writable
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.server.smithy.ServerCargoDependency
import software.amazon.smithy.rust.codegen.smithy.CoreCodegenContext
import software.amazon.smithy.rust.codegen.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.protocols.AwsJson
import software.amazon.smithy.rust.codegen.smithy.protocols.AwsJsonVersion
import software.amazon.smithy.rust.codegen.smithy.protocols.Protocol
import software.amazon.smithy.rust.codegen.smithy.protocols.RestJson
import software.amazon.smithy.rust.codegen.smithy.protocols.RestXml
import software.amazon.smithy.rust.codegen.util.orNull

fun allOperationShapes(service: ServiceShape, model: Model): List<OperationShape> {
    val resourceOperationShapes = service
        .resources
        .mapNotNull { model.getShape(it).orNull() }
        .mapNotNull { it as? ResourceShape }
        .flatMap { it.allOperations }
        .mapNotNull { model.getShape(it).orNull() }
        .mapNotNull { it as? OperationShape }
    val operationShapes = service.operations.mapNotNull { model.getShape(it).orNull() }.mapNotNull { it as? OperationShape }
    return resourceOperationShapes + operationShapes
}

interface ServerProtocol : Protocol {
    /** Returns the Rust marker struct enjoying `OperationShape`. */
    fun markerStruct(): RuntimeType

    /** Returns the Rust router type. */
    fun routerType(): RuntimeType

    // TODO(Decouple): Perhaps this should lean on a Rust interface.
    /**
     * Returns the construction of the `routerType` given a `ServiceShape`, a collection of operation values
     * (`self.operation_name`, ...), and the `Model`.
     */
    fun routerConstruction(service: ServiceShape, operationValues: Iterable<Writable>, model: Model): Writable

    companion object {
        /** Upgrades the core protocol to a `ServerProtocol`. */
        fun fromCoreProtocol(protocol: Protocol): ServerProtocol = when (protocol) {
            is AwsJson -> ServerAwsJsonProtocol.fromCoreProtocol(protocol)
            is RestJson -> ServerRestJsonProtocol.fromCoreProtocol(protocol)
            is RestXml -> ServerRestXmlProtocol.fromCoreProtocol(protocol)
            else -> throw IllegalStateException("unsupported protocol")
        }
    }
}

class ServerAwsJsonProtocol(
    coreCodegenContext: CoreCodegenContext,
    awsJsonVersion: AwsJsonVersion,
) : AwsJson(coreCodegenContext, awsJsonVersion), ServerProtocol {
    private val runtimeConfig = coreCodegenContext.runtimeConfig
    private val codegenScope = arrayOf(
        "SmithyHttpServer" to ServerCargoDependency.SmithyHttpServer(runtimeConfig).asType(),
    )
    private val symbolProvider = coreCodegenContext.symbolProvider

    companion object {
        fun fromCoreProtocol(awsJson: AwsJson): ServerAwsJsonProtocol = ServerAwsJsonProtocol(awsJson.coreCodegenContext, awsJson.version)
    }

    override fun markerStruct(): RuntimeType {
        val name = when (version) {
            is AwsJsonVersion.Json10 -> {
                "AwsJson10"
            }
            is AwsJsonVersion.Json11 -> {
                "AwsJson11"
            }
            else -> throw IllegalStateException()
        }
        return RuntimeType(name, ServerCargoDependency.SmithyHttpServer(runtimeConfig), "${runtimeConfig.crateSrcPrefix}_http_server::protocols")
    }

    override fun routerType() = RuntimeType("AwsJsonRouter", ServerCargoDependency.SmithyHttpServer(runtimeConfig), "${runtimeConfig.crateSrcPrefix}_http_server::routing::routers::aws_json")

    override fun routerConstruction(service: ServiceShape, operationValues: Iterable<Writable>, model: Model): Writable = writable {
        val allOperationShapes = allOperationShapes(service, model)

        // TODO(restore): This causes a panic: "symbol visitor should not be invoked in service shapes"
        // val serviceName = symbolProvider.toSymbol(service).name
        val serviceName = service.id.name
        val pairs = writable {
            for ((operation, operationValue) in allOperationShapes.zip(operationValues)) {
                val operationName = symbolProvider.toSymbol(operation).name
                rustTemplate(
                    """
                    (
                        String::from(""$serviceName.$operationName""),
                        #{SmithyHttpServer}::routing::Route::new(#{OperationValue:W})
                    ),
                    """,
                    "OperationValue" to operationValue,
                    *codegenScope,
                )
            }
        }
        rustTemplate(
            """
            #{Router}::from_iter([#{Pairs:W}])
            """,
            "Router" to routerType(),
            "Pairs" to pairs,
        )
    }
}

private fun restRouterType(runtimeConfig: RuntimeConfig) = RuntimeType("RestRouter", ServerCargoDependency.SmithyHttpServer(runtimeConfig), "${runtimeConfig.crateSrcPrefix}_http_server::routing::routers::rest")

private fun restRouterConstruction(
    protocol: ServerProtocol,
    service: ServiceShape,
    operationValues: Iterable<Writable>,
    model: Model,
    coreCodegenContext: CoreCodegenContext,
): Writable = writable {
    val allOperationShapes = allOperationShapes(service, model)

    // TODO(restore): This causes a panic: "symbol visitor should not be invoked in service shapes"
    // val serviceName = symbolProvider.toSymbol(service).name
    val serviceName = service.id.name
    val pairs = writable {
        for ((operationShape, operationValue) in allOperationShapes.zip(operationValues)) {
            val operationName = coreCodegenContext.symbolProvider.toSymbol(operationShape).name
            val key = protocol.serverRouterRequestSpec(
                operationShape,
                operationName,
                serviceName,
                ServerCargoDependency.SmithyHttpServer(coreCodegenContext.runtimeConfig).asType().member("routing::request_spec"),
            )
            rustTemplate(
                """
                (
                    #{Key:W},
                    #{SmithyHttpServer}::routing::Route::new(#{OperationValue:W})
                ),
                """,
                "Key" to key,
                "OperationValue" to operationValue,
                "SmithyHttpServer" to ServerCargoDependency.SmithyHttpServer(coreCodegenContext.runtimeConfig).asType(),
            )
        }
    }
    rustTemplate(
        """
        #{Router}::from_iter([#{Pairs:W}])
        """,
        "Router" to protocol.routerType(),
        "Pairs" to pairs,
    )
}

class ServerRestJsonProtocol(
    coreCodegenContext: CoreCodegenContext,
) : RestJson(coreCodegenContext), ServerProtocol {
    val runtimeConfig = coreCodegenContext.runtimeConfig

    companion object {
        fun fromCoreProtocol(restJson: RestJson): ServerRestJsonProtocol = ServerRestJsonProtocol(restJson.coreCodegenContext)
    }

    override fun markerStruct() = RuntimeType("AwsRestJson1", ServerCargoDependency.SmithyHttpServer(runtimeConfig), "${runtimeConfig.crateSrcPrefix}_http_server::protocols")

    override fun routerType() = restRouterType(runtimeConfig)

    override fun routerConstruction(service: ServiceShape, operationValues: Iterable<Writable>, model: Model): Writable = restRouterConstruction(this, service, operationValues, model, coreCodegenContext)
}

class ServerRestXmlProtocol(
    coreCodegenContext: CoreCodegenContext,
) : RestXml(coreCodegenContext), ServerProtocol {
    val runtimeConfig = coreCodegenContext.runtimeConfig

    companion object {
        fun fromCoreProtocol(restXml: RestXml): ServerRestXmlProtocol {
            return ServerRestXmlProtocol(restXml.coreCodegenContext)
        }
    }

    override fun markerStruct() = RuntimeType("AwsRestXml", ServerCargoDependency.SmithyHttpServer(runtimeConfig), "${runtimeConfig.crateSrcPrefix}_http_server::protocols")

    override fun routerType() = restRouterType(runtimeConfig)

    override fun routerConstruction(service: ServiceShape, operationValues: Iterable<Writable>, model: Model): Writable = restRouterConstruction(this, service, operationValues, model, coreCodegenContext)
}
