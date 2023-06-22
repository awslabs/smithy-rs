/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.customizations

import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.ClientRustModule
import software.amazon.smithy.rust.codegen.client.smithy.generators.ServiceRuntimePluginCustomization
import software.amazon.smithy.rust.codegen.client.smithy.generators.ServiceRuntimePluginSection
import software.amazon.smithy.rust.codegen.client.smithy.generators.config.ConfigCustomization
import software.amazon.smithy.rust.codegen.client.smithy.generators.config.ServiceConfig
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.RustCrate

class ResiliencyConfigCustomization(codegenContext: ClientCodegenContext) : ConfigCustomization() {
    private val runtimeConfig = codegenContext.runtimeConfig
    private val runtimeMode = codegenContext.smithyRuntimeMode
    private val retryConfig = RuntimeType.smithyTypes(runtimeConfig).resolve("retry")
    private val sleepModule = RuntimeType.smithyAsync(runtimeConfig).resolve("rt::sleep")
    private val timeoutModule = RuntimeType.smithyTypes(runtimeConfig).resolve("timeout")
    private val moduleUseName = codegenContext.moduleUseName()
    private val codegenScope = arrayOf(
        "RetryConfig" to retryConfig.resolve("RetryConfig"),
        "SharedAsyncSleep" to sleepModule.resolve("SharedAsyncSleep"),
        "Sleep" to sleepModule.resolve("Sleep"),
        "TimeoutConfig" to timeoutModule.resolve("TimeoutConfig"),
    )

    override fun section(section: ServiceConfig) =
        writable {
            when (section) {
                is ServiceConfig.ConfigStruct -> {
                    if (runtimeMode.defaultToMiddleware) {
                        rustTemplate(
                            """
                            retry_config: Option<#{RetryConfig}>,
                            sleep_impl: Option<#{SharedAsyncSleep}>,
                            timeout_config: Option<#{TimeoutConfig}>,
                            """,
                            *codegenScope,
                        )
                    }
                }

                is ServiceConfig.ConfigImpl -> {
                    if (runtimeMode.defaultToOrchestrator) {
                        rustTemplate(
                            """
                            /// Return a reference to the retry configuration contained in this config, if any.
                            pub fn retry_config(&self) -> Option<&#{RetryConfig}> {
                                self.inner.load::<#{RetryConfig}>()
                            }

                            /// Return a cloned shared async sleep implementation from this config, if any.
                            pub fn sleep_impl(&self) -> Option<#{SharedAsyncSleep}> {
                                self.inner.load::<#{SharedAsyncSleep}>().cloned()
                            }

                            /// Return a reference to the timeout configuration contained in this config, if any.
                            pub fn timeout_config(&self) -> Option<&#{TimeoutConfig}> {
                                self.inner.load::<#{TimeoutConfig}>()
                            }
                            """,
                            *codegenScope,
                        )
                    } else {
                        rustTemplate(
                            """
                            /// Return a reference to the retry configuration contained in this config, if any.
                            pub fn retry_config(&self) -> Option<&#{RetryConfig}> {
                                self.retry_config.as_ref()
                            }

                            /// Return a cloned shared async sleep implementation from this config, if any.
                            pub fn sleep_impl(&self) -> Option<#{SharedAsyncSleep}> {
                                self.sleep_impl.clone()
                            }

                            /// Return a reference to the timeout configuration contained in this config, if any.
                            pub fn timeout_config(&self) -> Option<&#{TimeoutConfig}> {
                                self.timeout_config.as_ref()
                            }
                            """,
                            *codegenScope,
                        )
                    }
                }

                is ServiceConfig.BuilderStruct -> {
                    rustTemplate(
                        """
                        retry_config: Option<#{RetryConfig}>,
                        sleep_impl: Option<#{SharedAsyncSleep}>,
                        timeout_config: Option<#{TimeoutConfig}>,
                        """,
                        *codegenScope,
                    )
                }

                ServiceConfig.BuilderImpl ->
                    rustTemplate(
                        """
                        /// Set the retry_config for the builder
                        ///
                        /// ## Examples
                        /// ```no_run
                        /// use $moduleUseName::config::Config;
                        /// use $moduleUseName::config::retry::RetryConfig;
                        ///
                        /// let retry_config = RetryConfig::standard().with_max_attempts(5);
                        /// let config = Config::builder().retry_config(retry_config).build();
                        /// ```
                        pub fn retry_config(mut self, retry_config: #{RetryConfig}) -> Self {
                            self.set_retry_config(Some(retry_config));
                            self
                        }

                        /// Set the retry_config for the builder
                        ///
                        /// ## Examples
                        /// ```no_run
                        /// use $moduleUseName::config::{Builder, Config};
                        /// use $moduleUseName::config::retry::RetryConfig;
                        ///
                        /// fn disable_retries(builder: &mut Builder) {
                        ///     let retry_config = RetryConfig::standard().with_max_attempts(1);
                        ///     builder.set_retry_config(Some(retry_config));
                        /// }
                        ///
                        /// let mut builder = Config::builder();
                        /// disable_retries(&mut builder);
                        /// let config = builder.build();
                        /// ```
                        pub fn set_retry_config(&mut self, retry_config: Option<#{RetryConfig}>) -> &mut Self {
                            self.retry_config = retry_config;
                            self
                        }

                        /// Set the sleep_impl for the builder
                        ///
                        /// ## Examples
                        ///
                        /// ```no_run
                        /// use $moduleUseName::config::{AsyncSleep, Config, SharedAsyncSleep, Sleep};
                        ///
                        /// ##[derive(Debug)]
                        /// pub struct ForeverSleep;
                        ///
                        /// impl AsyncSleep for ForeverSleep {
                        ///     fn sleep(&self, duration: std::time::Duration) -> Sleep {
                        ///         Sleep::new(std::future::pending())
                        ///     }
                        /// }
                        ///
                        /// let sleep_impl = SharedAsyncSleep::new(ForeverSleep);
                        /// let config = Config::builder().sleep_impl(sleep_impl).build();
                        /// ```
                        pub fn sleep_impl(mut self, sleep_impl: #{SharedAsyncSleep}) -> Self {
                            self.set_sleep_impl(Some(sleep_impl));
                            self
                        }

                        /// Set the sleep_impl for the builder
                        ///
                        /// ## Examples
                        ///
                        /// ```no_run
                        /// use $moduleUseName::config::{AsyncSleep, Builder, Config, SharedAsyncSleep, Sleep};
                        ///
                        /// ##[derive(Debug)]
                        /// pub struct ForeverSleep;
                        ///
                        /// impl AsyncSleep for ForeverSleep {
                        ///     fn sleep(&self, duration: std::time::Duration) -> Sleep {
                        ///         Sleep::new(std::future::pending())
                        ///     }
                        /// }
                        ///
                        /// fn set_never_ending_sleep_impl(builder: &mut Builder) {
                        ///     let sleep_impl = SharedAsyncSleep::new(ForeverSleep);
                        ///     builder.set_sleep_impl(Some(sleep_impl));
                        /// }
                        ///
                        /// let mut builder = Config::builder();
                        /// set_never_ending_sleep_impl(&mut builder);
                        /// let config = builder.build();
                        /// ```
                        pub fn set_sleep_impl(&mut self, sleep_impl: Option<#{SharedAsyncSleep}>) -> &mut Self {
                            self.sleep_impl = sleep_impl;
                            self
                        }

                        /// Set the timeout_config for the builder
                        ///
                        /// ## Examples
                        ///
                        /// ```no_run
                        /// ## use std::time::Duration;
                        /// use $moduleUseName::config::Config;
                        /// use $moduleUseName::config::timeout::TimeoutConfig;
                        ///
                        /// let timeout_config = TimeoutConfig::builder()
                        ///     .operation_attempt_timeout(Duration::from_secs(1))
                        ///     .build();
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
                        /// use $moduleUseName::config::timeout::TimeoutConfig;
                        ///
                        /// fn set_request_timeout(builder: &mut Builder) {
                        ///     let timeout_config = TimeoutConfig::builder()
                        ///         .operation_attempt_timeout(Duration::from_secs(1))
                        ///         .build();
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
                        *codegenScope,
                    )

                ServiceConfig.BuilderBuild -> {
                    if (runtimeMode.defaultToOrchestrator) {
                        rustTemplate(
                            """
                            self.retry_config.map(|r| layer.store_put(r));
                            self.sleep_impl.clone().map(|s| layer.store_put(s));
                            self.timeout_config.map(|t| layer.store_put(t));
                            """,
                            *codegenScope,
                        )
                    } else {
                        rustTemplate(
                            // We call clone on sleep_impl because the field is used by
                            // initializing the credentials_cache field later in the build
                            // method of a Config builder.
                            """
                            retry_config: self.retry_config,
                            sleep_impl: self.sleep_impl.clone(),
                            timeout_config: self.timeout_config,
                            """,
                            *codegenScope,
                        )
                    }
                }

                else -> emptySection
            }
        }
}

class ResiliencyReExportCustomization(private val runtimeConfig: RuntimeConfig) {
    fun extras(rustCrate: RustCrate) {
        rustCrate.withModule(ClientRustModule.Config) {
            rustTemplate(
                """
                pub use #{sleep}::{AsyncSleep, SharedAsyncSleep, Sleep};

                /// Retry configuration
                ///
                /// These are re-exported from `aws-smithy-types` for convenience.
                pub mod retry {
                    pub use #{types_retry}::{RetryConfig, RetryConfigBuilder, RetryMode};
                }
                /// Timeout configuration
                ///
                /// These are re-exported from `aws-smithy-types` for convenience.
                pub mod timeout {
                    pub use #{timeout}::{TimeoutConfig, TimeoutConfigBuilder};
                }
                """,
                "types_retry" to RuntimeType.smithyTypes(runtimeConfig).resolve("retry"),
                "sleep" to RuntimeType.smithyAsync(runtimeConfig).resolve("rt::sleep"),
                "timeout" to RuntimeType.smithyTypes(runtimeConfig).resolve("timeout"),
            )
        }
    }
}

class ResiliencyServiceRuntimePluginCustomization(private val codegenContext: ClientCodegenContext) : ServiceRuntimePluginCustomization() {
    private val runtimeConfig = codegenContext.runtimeConfig
    private val smithyRuntimeCrate = RuntimeType.smithyRuntime(runtimeConfig)
    private val retries = smithyRuntimeCrate.resolve("client::retries")
    private val codegenScope = arrayOf(
        "OnceCell" to RuntimeType.OnceCell.resolve("sync::OnceCell"),
        "TokenBucket" to retries.resolve("TokenBucket"),
        "ClientRateLimiter" to retries.resolve("ClientRateLimiter"),
        "RetryMode" to RuntimeType.smithyTypes(runtimeConfig).resolve("retry::RetryMode"),
        "StandardRetryStrategy" to retries.resolve("strategy::StandardRetryStrategy"),
        "SystemTime" to RuntimeType.std.resolve("time::SystemTime"),
    )

    override fun section(section: ServiceRuntimePluginSection): Writable = writable {
        when (section) {
            is ServiceRuntimePluginSection.AdditionalConfig -> {
                rustTemplate(
                    """
                    if let Some(sleep_impl) = self.handle.conf.sleep_impl() {
                        ${section.newLayerName}.put(sleep_impl);
                    }

                    match retry_config.mode() {
                        #{RetryMode}::Adaptive => {
                            let seconds_since_unix_epoch = self
                                .handle
                                .conf
                                .time_source()
                                .now()
                                .duration_since(#{SystemTime}::UNIX_EPOCH)
                                .expect("the present takes place after the UNIX_EPOCH")
                                .as_secs_f64();
                            let client_rate_limiter = CLIENT_RATE_LIMITER.get_or_init(|| {
                                #{ClientRateLimiter}::new(seconds_since_unix_epoch)
                            }).clone();
                            ${section.newLayerName}.put(client_rate_limiter);
                            // TODO(enableNewSmithyRuntimeLaunch) Do we need to insert the token bucket for adaptive retries?
                        },
                        #{RetryMode}::Standard => {
                            let token_bucket = TOKEN_BUCKET.get_or_init(#{TokenBucket}::default).clone();
                            ${section.newLayerName}.put(token_bucket);
                        },
                        _ => unreachable!("RetryMode is non-exhaustive")
                    }

                    ${section.newLayerName}.set_retry_strategy(#{StandardRetryStrategy}::new(&retry_config));
                    ${section.newLayerName}.put(self.handle.conf.time_source()#{maybe_clone})
                        .put(timeout_config)
                        .put(retry_config);
                    """,
                    *codegenScope,
                    "maybe_clone" to writable {
                        if (codegenContext.smithyRuntimeMode.defaultToMiddleware) {
                            rust(".clone()")
                        }
                    },
                )
            }
            is ServiceRuntimePluginSection.DeclareSingletons -> {
                // TODO(enableNewSmithyRuntimeCleanup) We can use the standard library's `OnceCell` once we upgrade the
                //    MSRV to 1.70
                rustTemplate(
                    """
                    static TOKEN_BUCKET: #{OnceCell}<#{TokenBucket}> = #{OnceCell}::new();
                    static CLIENT_RATE_LIMITER: #{OnceCell}<#{ClientRateLimiter}> = #{OnceCell}::new();
                    """,
                    *codegenScope,
                )
            }
            else -> emptySection
        }
    }
}
