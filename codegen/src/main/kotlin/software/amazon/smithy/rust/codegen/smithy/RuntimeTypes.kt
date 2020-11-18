/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.node.ObjectNode
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.lang.CargoDependency
import software.amazon.smithy.rust.codegen.lang.InlineDependency
import software.amazon.smithy.rust.codegen.lang.RustDependency
import software.amazon.smithy.rust.codegen.lang.RustType
import software.amazon.smithy.rust.codegen.lang.RustWriter
import java.io.File
import java.util.Optional

data class RuntimeConfig(val cratePrefix: String = "smithy", val relativePath: String = "../") {
    companion object {

        fun fromNode(node: Optional<ObjectNode>): RuntimeConfig {
            return if (node.isPresent) {
                RuntimeConfig(
                    node.get().getStringMemberOrDefault("cratePrefix", "smithy"),
                    File(node.get().getStringMemberOrDefault("relativePath", "../")).absolutePath
                )
            } else {
                RuntimeConfig()
            }
        }
    }
}

data class RuntimeType(val name: String, val dependency: RustDependency?, val namespace: String) {
    fun toSymbol(): Symbol {
        val builder = Symbol.builder().name(name).namespace(namespace, "::")
            .rustType(RustType.Opaque(name))

        dependency?.run { builder.addDependency(this) }
        return builder.build()
    }

    fun fullyQualifiedName(): String {
        val prefix = if (namespace.startsWith("crate")) {
            ""
        } else {
            "::"
        }
        return "$prefix$namespace::$name"
    }

    // TODO: refactor to be RuntimeTypeProvider a la Symbol provider that packages the `RuntimeConfig` state.
    companion object {

        // val Blob = RuntimeType("Blob", RustDependency.IO_CORE, "blob")
        val From = RuntimeType("From", dependency = null, namespace = "std::convert")
        val AsRef = RuntimeType("AsRef", dependency = null, namespace = "std::convert")
        fun StdFmt(member: String) = RuntimeType("fmt::$member", dependency = null, namespace = "std")
        fun Std(member: String) = RuntimeType(member, dependency = null, namespace = "std")
        val StdError = RuntimeType("Error", dependency = null, namespace = "std::error")
        val HashSet = RuntimeType("HashSet", dependency = null, namespace = "std::collections")
        val HashMap = RuntimeType("HashMap", dependency = null, namespace = "std::collections")

        fun Instant(runtimeConfig: RuntimeConfig) =
            RuntimeType("Instant", CargoDependency.SmithyTypes(runtimeConfig), "${runtimeConfig.cratePrefix}_types")

        fun Blob(runtimeConfig: RuntimeConfig) =
            RuntimeType("Blob", CargoDependency.SmithyTypes(runtimeConfig), "${runtimeConfig.cratePrefix}_types")

        fun Document(runtimeConfig: RuntimeConfig): RuntimeType =
            RuntimeType("Document", CargoDependency.SmithyTypes(runtimeConfig), "${runtimeConfig.cratePrefix}_types")

        fun LabelFormat(runtimeConfig: RuntimeConfig, func: String) =
            RuntimeType(func, CargoDependency.SmithyHttp(runtimeConfig), "${runtimeConfig.cratePrefix}_http::label")

        fun QueryFormat(runtimeConfig: RuntimeConfig, func: String) =
            RuntimeType(func, CargoDependency.SmithyHttp(runtimeConfig), "${runtimeConfig.cratePrefix}_http::query")

        fun Base64Encode(runtimeConfig: RuntimeConfig): RuntimeType =
            RuntimeType("encode", CargoDependency.SmithyHttp(runtimeConfig), "${runtimeConfig.cratePrefix}_http::base64")

        fun Base64Decode(runtimeConfig: RuntimeConfig): RuntimeType =
            RuntimeType("decode", CargoDependency.SmithyHttp(runtimeConfig), "${runtimeConfig.cratePrefix}_http::base64")

        fun TimestampFormat(runtimeConfig: RuntimeConfig, format: TimestampFormatTrait.Format): RuntimeType {
            val timestampFormat = when (format) {
                TimestampFormatTrait.Format.EPOCH_SECONDS -> "EpochSeconds"
                TimestampFormatTrait.Format.DATE_TIME -> "DateTime"
                TimestampFormatTrait.Format.HTTP_DATE -> "HttpDate"
                TimestampFormatTrait.Format.UNKNOWN -> TODO()
            }
            return RuntimeType(
                timestampFormat,
                CargoDependency.SmithyTypes(runtimeConfig),
                "${runtimeConfig.cratePrefix}_types::instant::Format"
            )
        }

        fun ProtocolTestHelper(runtimeConfig: RuntimeConfig, func: String): RuntimeType =
            RuntimeType(
                func, CargoDependency.ProtocolTestHelpers(runtimeConfig), "protocol_test_helpers"
            )


        fun Http(path: String): RuntimeType = RuntimeType(name = path, dependency = CargoDependency.Http, namespace = "http")
        val HttpRequestBuilder = Http("request::Builder")

        fun forInlineFun(name: String, module: String, func: (RustWriter) -> Unit) = RuntimeType(
            name = name,
            dependency = InlineDependency(name, module, func),
            namespace = "crate::$module"
        )
    }
}
