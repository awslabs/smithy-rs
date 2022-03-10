/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.customizations

import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.customize.RustCodegenDecorator
import software.amazon.smithy.rust.codegen.smithy.generators.config.ConfigCustomization
import software.amazon.smithy.rust.codegen.smithy.generators.config.ServiceConfig

/* Example Generated Code */
/*
pub struct Config {
    pub(crate) timeout_config: Option<aws_smithy_types::timeout::Config>,
}
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut config = f.debug_struct("Config");
        config.finish()
    }
}
impl Config {
    pub fn builder() -> Builder {
        Builder::default()
    }
}
#[derive(Default)]
pub struct Builder {
    timeout_config: Option<aws_smithy_types::timeout::Config>,
}
impl Builder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn timeout_config(mut self, timeout_config: aws_smithy_types::timeout::Config) -> Self {
        self.set_timeout_config(Some(timeout_config));
        self
    }
    pub fn set_timeout_config(
        &mut self,
        timeout_config: Option<aws_smithy_types::timeout::Config>,
    ) -> &mut Self {
        self.timeout_config = timeout_config;
        self
    }
    pub fn build(self) -> Config {
        Config {
            timeout_config: self.timeout_config,
        }
    }
}
#[test]
fn test_1() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Config>();
}

 */

class TimeoutConfigDecorator : RustCodegenDecorator {
    override val name: String = "TimeoutConfig"
    override val order: Byte = 0

    override fun configCustomizations(
        codegenContext: CodegenContext,
        baseCustomizations: List<ConfigCustomization>
    ): List<ConfigCustomization> {
        return baseCustomizations + TimeoutConfigProviderConfig(codegenContext)
    }
}

class TimeoutConfigProviderConfig(codegenContext: CodegenContext) : ConfigCustomization() {
    private val timeoutModule = smithyTypesTimeout(codegenContext.runtimeConfig)
    private val moduleUseName = codegenContext.moduleUseName()
    private val codegenScope = arrayOf("TimeoutConfig" to timeoutModule.member("Config"))
    override fun section(section: ServiceConfig) = writable {
        when (section) {
            is ServiceConfig.ConfigStruct -> rustTemplate(
                "pub(crate) timeout_config: Option<#{TimeoutConfig}>,",
                *codegenScope
            )
            is ServiceConfig.ConfigImpl -> emptySection
            is ServiceConfig.BuilderStruct ->
                rustTemplate("timeout_config: Option<#{TimeoutConfig}>,", *codegenScope)
            ServiceConfig.BuilderImpl ->
                rustTemplate(
                    """
                    /// Set the timeout_config for the builder
                    ///
                    /// ## Examples
                    ///
                    /// ```no_run
                    /// ## use std::time::Duration;
                    /// use $moduleUseName::config::Config;
                    /// use #{TimeoutConfig};
                    ///
                    /// let timeout_config = TimeoutConfig::new()
                    ///     .with_api_call_attempt_timeout(Some(Duration::from_secs(1)));
                    /// let config = Config::builder().timeout_config(timeout_config).build();
                    /// ```
                    pub fn timeout_config(mut self, timeout_config: #{TimeoutConfig}) -> Self {
                        self.set_timeout_config(Some(timeout_config));
                        self
                    }

                    /// Set the timeout_config for the builder
                    ///
                    /// ## Examples
                    ///
                    /// ```no_run
                    /// ## use std::time::Duration;
                    /// use $moduleUseName::config::{Builder, Config};
                    /// use #{TimeoutConfig};
                    ///
                    /// fn set_request_timeout(builder: &mut Builder) {
                    ///     let timeout_config = TimeoutConfig::new()
                    ///         .with_api_call_timeout(Some(Duration::from_secs(3)));
                    ///     builder.set_timeout_config(Some(timeout_config));
                    /// }
                    ///
                    /// let mut builder = Config::builder();
                    /// set_request_timeout(&mut builder);
                    /// let config = builder.build();
                    /// ```
                    pub fn set_timeout_config(&mut self, timeout_config: Option<#{TimeoutConfig}>) -> &mut Self {
                        self.timeout_config = timeout_config;
                        self
                    }
                    """,
                    *codegenScope
                )
            ServiceConfig.BuilderBuild -> rustTemplate(
                """timeout_config: self.timeout_config,""",
                *codegenScope
            )
        }
    }
}

// Generate path to the timeout module in aws_smithy_types
fun smithyTypesTimeout(runtimeConfig: RuntimeConfig) =
    RuntimeType("timeout", runtimeConfig.runtimeCrate("types"), "aws_smithy_types")
