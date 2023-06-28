/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.generators

import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.generators.config.ConfigCustomization
import software.amazon.smithy.rust.codegen.client.smithy.generators.config.ServiceConfig
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.customize.writeCustomizations

class ConfigOverrideRuntimePluginGenerator(
    codegenContext: ClientCodegenContext,
) {
    private val moduleUseName = codegenContext.moduleUseName()
    private val codegenScope = codegenContext.runtimeConfig.let { rc ->
        val runtimeApi = RuntimeType.smithyRuntimeApi(rc)
        val smithyTypes = RuntimeType.smithyTypes(rc)
        arrayOf(
            *RuntimeType.preludeScope,
            "CloneableLayer" to smithyTypes.resolve("config_bag::CloneableLayer"),
            "ConfigBagAccessors" to runtimeApi.resolve("client::config_bag_accessors::ConfigBagAccessors"),
            "FrozenLayer" to smithyTypes.resolve("config_bag::FrozenLayer"),
            "InterceptorRegistrar" to runtimeApi.resolve("client::interceptors::InterceptorRegistrar"),
            "Layer" to smithyTypes.resolve("config_bag::Layer"),
            "RuntimePlugin" to runtimeApi.resolve("client::runtime_plugin::RuntimePlugin"),
        )
    }

    fun render(writer: RustWriter, customizations: List<ConfigCustomization>) {
        writer.rustTemplate(
            """
            /// A plugin that enables configuration for a single operation invocation
            ///
            /// The `config` method will return a `FrozenLayer` by storing values from `config_override`.
            /// In the case of default values requested, they will be obtained from `client_config`.
            ##[derive(Debug)]
            pub(crate) struct ConfigOverrideRuntimePlugin {
                pub(crate) config_override: Builder,
                pub(crate) client_config: #{FrozenLayer},
            }

            impl #{RuntimePlugin} for ConfigOverrideRuntimePlugin {
                fn config(&self) -> #{Option}<#{FrozenLayer}> {
                    use #{ConfigBagAccessors};

                    ##[allow(unused_mut)]
                    let layer: #{Layer} = self
                        .config_override
                        .inner
                        .clone()
                        .into();
                    let mut layer = layer.with_name("$moduleUseName::config::ConfigOverrideRuntimePlugin");
                    #{config}

                    #{Some}(layer.freeze())
                }

                fn interceptors(&self, _interceptors: &mut #{InterceptorRegistrar}) {
                    #{interceptors}
                }
            }

            """,
            *codegenScope,
            "config" to writable {
                writeCustomizations(
                    customizations,
                    ServiceConfig.RuntimePluginConfig("layer"),
                )
            },
            "interceptors" to writable {
                writeCustomizations(customizations, ServiceConfig.RuntimePluginInterceptors("_interceptors", "self.config_override"))
            },
        )
    }
}
