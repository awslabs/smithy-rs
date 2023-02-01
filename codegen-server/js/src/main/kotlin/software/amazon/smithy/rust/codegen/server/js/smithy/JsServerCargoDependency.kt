/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.js.smithy

import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.core.rustlang.CratesIo
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeConfig

/**
 * Object used *exclusively* in the runtime of the Python server, for separation concerns.
 * Analogous to the companion object in [CargoDependency] and [software.amazon.smithy.rust.codegen.server.smithy.ServerCargoDependency]; see its documentation for details.
 * For a dependency that is used in the client, or in both the client and the server, use [CargoDependency] directly.
 */
object JsServerCargoDependency {
    val Napi: CargoDependency = CargoDependency("napi", CratesIo("2.10"), features = setOf("tokio_rt", "napi4"))
    val NapiDerive: CargoDependency = CargoDependency("napi-derive", CratesIo("2.10"))
    val PyO3: CargoDependency = CargoDependency("pyo3", CratesIo("0.17"))
    val PyO3Asyncio: CargoDependency = CargoDependency("pyo3-asyncio", CratesIo("0.17"), features = setOf("attributes", "tokio-runtime"))
    val Tokio: CargoDependency = CargoDependency("tokio", CratesIo("1.20.1"), features = setOf("full"))
    val Tracing: CargoDependency = CargoDependency("tracing", CratesIo("0.1"))
    val Tower: CargoDependency = CargoDependency("tower", CratesIo("0.4"))
    val TowerHttp: CargoDependency = CargoDependency("tower-http", CratesIo("0.3"), features = setOf("trace"))
    val Hyper: CargoDependency = CargoDependency("hyper", CratesIo("0.14.12"), features = setOf("server", "http1", "http2", "tcp", "stream"))
    val NumCpus: CargoDependency = CargoDependency("num_cpus", CratesIo("1.13"))
    val ParkingLot: CargoDependency = CargoDependency("parking_lot", CratesIo("0.12"))

    fun smithyHttpServer(runtimeConfig: RuntimeConfig) = runtimeConfig.smithyRuntimeCrate("smithy-http-server")
    fun smithyHttpServerJs(runtimeConfig: RuntimeConfig) = runtimeConfig.smithyRuntimeCrate("smithy-http-server-js")
    fun smithyHttpServerPython(runtimeConfig: RuntimeConfig) = runtimeConfig.smithyRuntimeCrate("smithy-http-server-python")
}
