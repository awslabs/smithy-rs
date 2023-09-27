/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.generators

import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.core.smithy.customize.NamedCustomization
import software.amazon.smithy.rust.codegen.core.smithy.customize.Section

sealed class OperationSection(name: String) : Section(name) {
    abstract val customizations: List<OperationCustomization>

    /** Write custom code into the `impl` block of this operation */
    data class OperationImplBlock(override val customizations: List<OperationCustomization>) :
        OperationSection("OperationImplBlock")

    data class MutateOutput(
        override val customizations: List<OperationCustomization>,
        val operationShape: OperationShape,
        /** Name of the response headers map (for referring to it in Rust code) */
        val responseHeadersName: String,
    ) : OperationSection("MutateOutput")

    /**
     * Allows for adding additional properties to the `extras` field on the
     * `aws_smithy_types::error::ErrorMetadata`.
     */
    data class PopulateErrorMetadataExtras(
        override val customizations: List<OperationCustomization>,
        /** Name of the generic error builder (for referring to it in Rust code) */
        val builderName: String,
        /** Name of the response status (for referring to it in Rust code) */
        val responseStatusName: String,
        /** Name of the response headers map (for referring to it in Rust code) */
        val responseHeadersName: String,
    ) : OperationSection("PopulateErrorMetadataExtras")

    /**
     * Hook to add custom code right before the response is parsed.
     */
    data class BeforeParseResponse(
        override val customizations: List<OperationCustomization>,
        val responseName: String,
        /**
         * Name of the `force_error` variable. Set this to true to trigger error parsing.
         */
        val forceError: String,
        /**
         * When set, the name of the response body data field
         */
        val body: String?,
    ) : OperationSection("BeforeParseResponse")

    /**
     * Hook for adding additional things to config inside operation runtime plugins.
     */
    data class AdditionalRuntimePluginConfig(
        override val customizations: List<OperationCustomization>,
        val newLayerName: String,
        val operationShape: OperationShape,
    ) : OperationSection("AdditionalRuntimePluginConfig")

    data class AdditionalInterceptors(
        override val customizations: List<OperationCustomization>,
        val operationShape: OperationShape,
    ) : OperationSection("AdditionalInterceptors") {
        fun registerInterceptor(runtimeConfig: RuntimeConfig, writer: RustWriter, interceptor: Writable) {
            writer.rustTemplate(
                ".with_interceptor(#{interceptor})",
                "interceptor" to interceptor,
            )
        }
    }

    /**
     * Hook for adding retry classifiers to an operation's `RetryClassifiers` bundle.
     *
     * Should emit 1+ lines of code that look like the following:
     * ```rust
     * .with_classifier(AwsErrorCodeClassifier::new())
     * .with_classifier(HttpStatusCodeClassifier::new())
     * ```
     */
    data class RetryClassifier(
        override val customizations: List<OperationCustomization>,
        val configBagName: String,
        val operationShape: OperationShape,
    ) : OperationSection("RetryClassifier")

    /**
     * Hook for adding supporting types for operation-specific runtime plugins.
     * Examples include various operation-specific types (retry classifiers, config bag types, etc.)
     */
    data class RuntimePluginSupportingTypes(
        override val customizations: List<OperationCustomization>,
        val configBagName: String,
        val operationShape: OperationShape,
    ) : OperationSection("RuntimePluginSupportingTypes")

    /**
     * Hook for adding additional runtime plugins to an operation.
     */
    data class AdditionalRuntimePlugins(
        override val customizations: List<OperationCustomization>,
        val operationShape: OperationShape,
    ) : OperationSection("AdditionalRuntimePlugins") {
        fun addServiceRuntimePlugin(writer: RustWriter, plugin: Writable) {
            writer.rustTemplate(".with_service_plugin(#{plugin})", "plugin" to plugin)
        }

        fun addOperationRuntimePlugin(writer: RustWriter, plugin: Writable) {
            writer.rustTemplate(".with_operation_plugin(#{plugin})", "plugin" to plugin)
        }
    }
}

abstract class OperationCustomization : NamedCustomization<OperationSection>()
