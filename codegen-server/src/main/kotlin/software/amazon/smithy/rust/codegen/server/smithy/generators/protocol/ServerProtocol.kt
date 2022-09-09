/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators.protocol

import software.amazon.smithy.model.knowledge.TopDownIndex
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.rust.codegen.rustlang.Writable
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.server.smithy.ServerCargoDependency
import software.amazon.smithy.rust.codegen.server.smithy.ServerRuntimeType
import software.amazon.smithy.rust.codegen.smithy.CoreCodegenContext
import software.amazon.smithy.rust.codegen.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.protocols.AwsJson
import software.amazon.smithy.rust.codegen.smithy.protocols.AwsJsonVersion
import software.amazon.smithy.rust.codegen.smithy.protocols.Protocol
import software.amazon.smithy.rust.codegen.smithy.protocols.RestJson
import software.amazon.smithy.rust.codegen.smithy.protocols.RestXml

private fun allOperations(coreCodegenContext: CoreCodegenContext): List<OperationShape> {
    val index = TopDownIndex.of(coreCodegenContext.model)
    return index.getContainedOperations(coreCodegenContext.serviceShape).sortedBy { it.id }
}

interface ServerProtocol : Protocol {
    /** Returns the Rust marker struct enjoying `OperationShape`. */
    fun markerStruct(): RuntimeType

    /** Returns the Rust router type. */
    fun routerType(): RuntimeType

    /**
     * Returns the construction of the `routerType` given a `ServiceShape`, a collection of operation values
     * (`self.operation_name`, ...), and the `Model`.
     */
    fun routerConstruction(operationValues: Iterable<Writable>): Writable

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
    private val service = coreCodegenContext.serviceShape

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
        }
        return ServerRuntimeType.Protocol(name, runtimeConfig)
    }

    override fun routerType() = RuntimeType("AwsJsonRouter", ServerCargoDependency.SmithyHttpServer(runtimeConfig), "${runtimeConfig.crateSrcPrefix}_http_server::routing::routers::aws_json")

    override fun routerConstruction(operationValues: Iterable<Writable>): Writable = writable {
        val allOperationShapes = allOperations(coreCodegenContext)

        // TODO(https://github.com/awslabs/smithy-rs/issues/1724#issue-1367509999): This causes a panic: "symbol
        // visitor should not be invoked in service shapes"
        // val serviceName = symbolProvider.toSymbol(service).name
        val serviceName = service.id.name
        val pairs = writable {
            for ((operation, operationValue) in allOperationShapes.zip(operationValues)) {
                val operationName = symbolProvider.toSymbol(operation).name
                rustTemplate(
                    """
                    (
                        String::from("$serviceName.$operationName"),
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
    operationValues: Iterable<Writable>,
    coreCodegenContext: CoreCodegenContext,
): Writable = writable {
    val operations = allOperations(coreCodegenContext)

    // TODO(https://github.com/awslabs/smithy-rs/issues/1724#issue-1367509999): This causes a panic: "symbol visitor
    // should not be invoked in service shapes"
    // val serviceName = symbolProvider.toSymbol(service).name
    val serviceName = coreCodegenContext.serviceShape.id.name
    val pairs = writable {
        for ((operationShape, operationValue) in operations.zip(operationValues)) {
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

    override fun markerStruct() = ServerRuntimeType.Protocol("AwsRestJson1", runtimeConfig)

    override fun routerType() = restRouterType(runtimeConfig)

    override fun routerConstruction(operationValues: Iterable<Writable>): Writable = restRouterConstruction(this, operationValues, coreCodegenContext)
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

    override fun markerStruct() = ServerRuntimeType.Protocol("AwsRestXml", runtimeConfig)

    override fun routerType() = restRouterType(runtimeConfig)

    override fun routerConstruction(operationValues: Iterable<Writable>): Writable = restRouterConstruction(this, operationValues, coreCodegenContext)
}
