/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.core.smithy

import software.amazon.smithy.codegen.core.CodegenException
import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.node.Node
import software.amazon.smithy.model.node.ObjectNode
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.core.Version
import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.core.rustlang.CratesIo
import software.amazon.smithy.rust.codegen.core.rustlang.DependencyLocation
import software.amazon.smithy.rust.codegen.core.rustlang.DependencyScope
import software.amazon.smithy.rust.codegen.core.rustlang.InlineDependency
import software.amazon.smithy.rust.codegen.core.rustlang.Local
import software.amazon.smithy.rust.codegen.core.rustlang.RustDependency
import software.amazon.smithy.rust.codegen.core.rustlang.RustModule
import software.amazon.smithy.rust.codegen.core.rustlang.RustType
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rustInlineTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.util.orNull
import java.util.Optional

private const val DEFAULT_KEY = "DEFAULT"

/**
 * Location of the runtime crates (aws-smithy-http, aws-smithy-types etc.)
 *
 * This can be configured via the `runtimeConfig.versions` field in smithy-build.json
 */
data class RuntimeCrateLocation(val path: String?, val versions: CrateVersionMap) {
    companion object {
        fun Path(path: String) = RuntimeCrateLocation(path, CrateVersionMap(emptyMap()))
    }
}

fun RuntimeCrateLocation.crateLocation(crateName: String?): DependencyLocation {
    val version = crateName.let { versions.map[crateName] } ?: versions.map[DEFAULT_KEY]
    return when (this.path) {
        // CratesIo needs an exact version. However, for local runtime crates we do not
        // provide a detected version unless the user explicitly sets one via the `versions` map.
        null -> CratesIo(version ?: defaultRuntimeCrateVersion())
        else -> Local(this.path, version)
    }
}

fun defaultRuntimeCrateVersion(): String {
    try {
        return Version.crateVersion()
    } catch (ex: Exception) {
        throw CodegenException("failed to get crate version which sets the default client-runtime version", ex)
    }
}

/**
 * A mapping from crate name to a user-specified version.
 */
@JvmInline
value class CrateVersionMap(
    val map: Map<String, String>,
)

/**
 * Prefix & crate location for the runtime crates.
 */
data class RuntimeConfig(
    val cratePrefix: String = "aws",
    val runtimeCrateLocation: RuntimeCrateLocation = RuntimeCrateLocation.Path("../"),
) {
    companion object {

        /**
         * Load a `RuntimeConfig` from an [ObjectNode] (JSON)
         */
        fun fromNode(maybeNode: Optional<ObjectNode>): RuntimeConfig {
            val node = maybeNode.orElse(Node.objectNode())
            val crateVersionMap = node.getObjectMember("versions").orElse(Node.objectNode()).members.entries.let { members ->
                val map = members.associate { it.key.toString() to it.value.expectStringNode().value }
                CrateVersionMap(map)
            }
            val path = node.getStringMember("relativePath").orNull()?.value
            val runtimeCrateLocation = RuntimeCrateLocation(path = path, versions = crateVersionMap)
            return RuntimeConfig(
                node.getStringMemberOrDefault("cratePrefix", "aws"),
                runtimeCrateLocation = runtimeCrateLocation,
            )
        }
    }

    val crateSrcPrefix: String = cratePrefix.replace("-", "_")

    fun smithyRuntimeCrate(runtimeCrateName: String, optional: Boolean = false, scope: DependencyScope = DependencyScope.Compile): CargoDependency {
        val crateName = "$cratePrefix-$runtimeCrateName"
        return CargoDependency(
            crateName,
            runtimeCrateLocation.crateLocation(crateName),
            optional = optional,
            scope = scope,
        )
    }
}

/**
 * `RuntimeType` captures all necessary information to render a type into a Rust file:
 * - [name]: What type is this?
 * - [namespace]: Where can we find this type.
 * - [dependency]: What other crates, if any, are required to use this type?
 *
 * For example:
 *
 * `RuntimeType("header::HeaderName", CargoDependency.Http)`, when passed to a [RustWriter] would appear as such:
 *
 * `http::header::HeaderName`
 *  ------------  ----------
 *       |            |
 *  `[namespace]` `[name]`
 *
 *  This type would have a [CargoDependency] pointing to the `http` crate. Writing it multiple times would still only
 *  add the dependency once.
 */
data class RuntimeType(val path: String, val dependency: RustDependency? = null) {
    val name: String
    val namespace: String

    init {
        val splitPath = path.split("::").toMutableList()
        // get the name at the end
        this.name = splitPath.removeLast()
        // get all parts that aren't the name at the end
        this.namespace = splitPath.joinToString("::")
    }

    /**
     * Get a writable for this `RuntimeType`
     */
    val writable = writable {
        rustInlineTemplate(
            "#{this:T}",
            "this" to this@RuntimeType,
        )
    }

    /**
     * Convert this [RuntimeType] into a [Symbol].
     *
     * This is not commonly required, but is occasionally useful when you want to force an import without referencing a type
     * (e.g. when bringing a trait into scope). See [CodegenWriter.addUseImports].
     */
    fun toSymbol(): Symbol {
        val builder = Symbol
            .builder()
            .name(name)
            .namespace(namespace, "::")
            .rustType(RustType.Opaque(name, namespace))

        dependency?.run { builder.addDependency(this) }
        return builder.build()
    }

    /**
     * Create a new [RuntimeType] with a nested path.
     *
     * # Example
     * ```kotlin
     * val http = CargoDependency.http.resolve("Request")
     * ```
     */
    fun resolve(subPath: String): RuntimeType {
        return copy(path = "$path::$subPath")
    }

    /**
     * Returns the fully qualified name for this type
     */
    fun fullyQualifiedName(): String {
        return path
    }

    /**
     * The companion object contains commonly used RuntimeTypes
     */
    companion object {
        // stdlib types
        val std = RuntimeType("std")
        val stdFmt = std.resolve("fmt")
        val AsRef = RuntimeType("std::convert::AsRef")
        val ByteSlab = RuntimeType("std::vec::Vec<u8>")
        val Clone = std.resolve("clone::Clone")
        val Debug = stdFmt.resolve("Debug")
        val Default = RuntimeType("std::default::Default")
        val Display = stdFmt.resolve("Display")
        val From = RuntimeType("std::convert::From")
        val TryFrom = RuntimeType("std::convert::TryFrom")
        val PartialEq = std.resolve("cmp::PartialEq")
        val StdError = RuntimeType("std::error::Error")
        val String = RuntimeType("std::string::String")
        val Phantom = RuntimeType("std::marker::PhantomData")
        val Cow = RuntimeType("std::borrow::Cow")

        // codegen types
        val Config = RuntimeType("crate::config")

        // smithy runtime types
        fun smithyAsync(runtimeConfig: RuntimeConfig) = CargoDependency.smithyAsync(runtimeConfig).asType()
        fun smithyChecksums(runtimeConfig: RuntimeConfig) = CargoDependency.smithyChecksums(runtimeConfig).asType()
        fun smithyClient(runtimeConfig: RuntimeConfig) = CargoDependency.smithyClient(runtimeConfig).asType()
        fun smithyEventstream(runtimeConfig: RuntimeConfig) = CargoDependency.smithyEventstream(runtimeConfig).asType()
        fun smithyHttp(runtimeConfig: RuntimeConfig) = CargoDependency.smithyHttp(runtimeConfig).asType()
        fun smithyJson(runtimeConfig: RuntimeConfig) = CargoDependency.smithyJson(runtimeConfig).asType()
        fun smithyQuery(runtimeConfig: RuntimeConfig) = CargoDependency.smithyQuery(runtimeConfig).asType()
        fun smithyTypes(runtimeConfig: RuntimeConfig) = CargoDependency.smithyTypes(runtimeConfig).asType()
        fun smithyXml(runtimeConfig: RuntimeConfig) = CargoDependency.smithyXml(runtimeConfig).asType()
        fun smithyProtocolTest(runtimeConfig: RuntimeConfig) = CargoDependency.smithyProtocolTestHelpers(runtimeConfig).asType()

        // smithy runtime type members
        fun base64Decode(runtimeConfig: RuntimeConfig): RuntimeType = smithyTypes(runtimeConfig).resolve("base64::decode")
        fun base64Encode(runtimeConfig: RuntimeConfig): RuntimeType = smithyTypes(runtimeConfig).resolve("base64::encode")
        fun blob(runtimeConfig: RuntimeConfig) = smithyTypes(runtimeConfig).resolve("Blob")
        fun byteStream(runtimeConfig: RuntimeConfig) = smithyHttp(runtimeConfig).resolve("byte_stream::ByteStream")
        fun dateTime(runtimeConfig: RuntimeConfig) = smithyTypes(runtimeConfig).resolve("DateTime")
        fun document(runtimeConfig: RuntimeConfig): RuntimeType = smithyTypes(runtimeConfig).resolve("Document")
        fun errorKind(runtimeConfig: RuntimeConfig) = smithyTypes(runtimeConfig).resolve("retry::ErrorKind")
        fun eventStreamReceiver(runtimeConfig: RuntimeConfig): RuntimeType = smithyHttp(runtimeConfig).resolve("event_stream::Receiver")
        fun genericError(runtimeConfig: RuntimeConfig) = smithyTypes(runtimeConfig).resolve("Error")
        fun jsonErrors(runtimeConfig: RuntimeConfig) = forInlineDependency(InlineDependency.jsonErrors(runtimeConfig))
        fun labelFormat(runtimeConfig: RuntimeConfig, func: String) = smithyHttp(runtimeConfig).resolve("label::$func")
        fun operation(runtimeConfig: RuntimeConfig) = smithyHttp(runtimeConfig).resolve("operation::Operation")
        fun operationModule(runtimeConfig: RuntimeConfig) = smithyHttp(runtimeConfig).resolve("operation")
        fun parseResponse(runtimeConfig: RuntimeConfig) = smithyHttp(runtimeConfig).resolve("response::ParseHttpResponse")
        fun parseStrictResponse(runtimeConfig: RuntimeConfig) = smithyHttp(runtimeConfig).resolve("response::ParseStrictResponse")
        fun protocolTest(runtimeConfig: RuntimeConfig, func: String): RuntimeType = smithyProtocolTest(runtimeConfig).resolve(func)
        fun provideErrorKind(runtimeConfig: RuntimeConfig) = smithyTypes(runtimeConfig).resolve("retry::ProvideErrorKind")
        fun queryFormat(runtimeConfig: RuntimeConfig, func: String) = smithyHttp(runtimeConfig).resolve("query::$func")
        fun sdkBody(runtimeConfig: RuntimeConfig): RuntimeType = smithyHttp(runtimeConfig).resolve("body::SdkBody")
        fun timestampFormat(runtimeConfig: RuntimeConfig, format: TimestampFormatTrait.Format): RuntimeType {
            val timestampFormat = when (format) {
                TimestampFormatTrait.Format.EPOCH_SECONDS -> "EpochSeconds"
                TimestampFormatTrait.Format.DATE_TIME -> "DateTime"
                TimestampFormatTrait.Format.HTTP_DATE -> "HttpDate"
                TimestampFormatTrait.Format.UNKNOWN -> TODO()
            }

            return smithyTypes(runtimeConfig).resolve("date_time::Format::$timestampFormat")
        }

        fun forInlineDependency(inlineDependency: InlineDependency) = RuntimeType("crate::${inlineDependency.name}", inlineDependency)

        fun forInlineFun(name: String, module: RustModule, func: Writable) = RuntimeType(
            "crate::${module.name}::$name",
            dependency = InlineDependency(name, module, listOf(), func),
        )

        // inlinable types
        fun ec2QueryErrors(runtimeConfig: RuntimeConfig) =
            forInlineDependency(InlineDependency.ec2QueryErrors(runtimeConfig))
        fun wrappedXmlErrors(runtimeConfig: RuntimeConfig) =
            forInlineDependency(InlineDependency.wrappedXmlErrors(runtimeConfig))
        fun unwrappedXmlErrors(runtimeConfig: RuntimeConfig) =
            forInlineDependency(InlineDependency.unwrappedXmlErrors(runtimeConfig))
        val IdempotencyToken by lazy { forInlineDependency(InlineDependency.idempotencyToken()) }
    }
}
