/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.customizations

import software.amazon.smithy.rust.codegen.rustlang.Writable
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.customize.RustCodegenDecorator
import software.amazon.smithy.rust.codegen.smithy.generators.LibRsCustomization
import software.amazon.smithy.rust.codegen.smithy.generators.LibRsSection
import software.amazon.smithy.rust.codegen.smithy.generators.config.ConfigCustomization
import software.amazon.smithy.rust.codegen.smithy.generators.config.ServiceConfig

/* Example Generated Code */
/*
pub struct Config {
    pub(crate) sleep_impl: Option<Arc<dyn AsyncSleep>>,
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
    sleep_impl: Option<Arc<dyn AsyncSleep>>,
}
impl Builder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn sleep_impl(mut self, sleep_impl: Arc<dyn AsyncSleep>) -> Self {
        self.set_sleep_impl(Some(sleep_impl));
        self
    }
    pub fn set_sleep_impl(
        &mut self,
        sleep_impl: Option<Arc<dyn AsyncSleep>>,
    ) -> &mut Self {
        self.sleep_impl = sleep_impl;
        self
    }
    pub fn build(self) -> Config {
        Config {
            sleep_impl: self.sleep_impl,
        }
    }
}
#[test]
fn test_1() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Config>();
}
 */

class SleepImplDecorator : RustCodegenDecorator {
    override val name: String = "AsyncSleep"
    override val order: Byte = 0

    override fun configCustomizations(
        codegenContext: CodegenContext,
        baseCustomizations: List<ConfigCustomization>
    ): List<ConfigCustomization> {
        return baseCustomizations + SleepImplProviderConfig(codegenContext)
    }

    override fun libRsCustomizations(
        codegenContext: CodegenContext,
        baseCustomizations: List<LibRsCustomization>
    ): List<LibRsCustomization> {
        return baseCustomizations + PubUseAsyncSleep(codegenContext.runtimeConfig)
    }
}

class SleepImplProviderConfig(codegenContext: CodegenContext) : ConfigCustomization() {
    private val sleepModule = smithyAsyncRtSleep(codegenContext.runtimeConfig)
    private val moduleName = codegenContext.moduleName
    private val moduleUseName = moduleName.replace("-", "_")
    private val codegenScope =
        arrayOf("AsyncSleep" to sleepModule.member("AsyncSleep"), "Sleep" to sleepModule.member("Sleep"))

    override fun section(section: ServiceConfig) = writable {
        when (section) {
            is ServiceConfig.ConfigStruct -> rustTemplate(
                "pub(crate) sleep_impl: Option<std::sync::Arc<dyn #{AsyncSleep}>>,",
                *codegenScope
            )
            is ServiceConfig.ConfigImpl -> emptySection
            is ServiceConfig.BuilderStruct ->
                rustTemplate("sleep_impl: Option<std::sync::Arc<dyn #{AsyncSleep}>>,", *codegenScope)
            ServiceConfig.BuilderImpl ->
                rustTemplate(
                    """
                    /// Set the sleep_impl for the builder
                    ///
                    /// ## Examples
                    /// ```rust,no_run
                    /// use $moduleUseName::config::Config;
                    /// use #{AsyncSleep};
                    /// use #{Sleep};
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
                    /// let sleep_impl = std::sync::Arc::new(ForeverSleep);
                    /// let config = Config::builder().sleep_impl(sleep_impl).build();
                    /// ```
                    pub fn sleep_impl(mut self, sleep_impl: std::sync::Arc<dyn #{AsyncSleep}>) -> Self {
                        self.set_sleep_impl(Some(sleep_impl));
                        self
                    }

                    /// Set the sleep_impl for the builder
                    ///
                    /// ## Examples
                    /// ```rust,no_run
                    /// use $moduleUseName::config::{Builder, Config};
                    /// use #{AsyncSleep};
                    /// use #{Sleep};
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
                    ///     let sleep_impl = std::sync::Arc::new(ForeverSleep);
                    ///     builder.set_sleep_impl(Some(sleep_impl));
                    /// }
                    ///
                    /// let mut builder = Config::builder();
                    /// set_never_ending_sleep_impl(&mut builder);
                    /// let config = builder.build();
                    /// ```
                    pub fn set_sleep_impl(&mut self, sleep_impl: Option<std::sync::Arc<dyn #{AsyncSleep}>>) -> &mut Self {
                        self.sleep_impl = sleep_impl;
                        self
                    }
                    """,
                    *codegenScope
                )
            ServiceConfig.BuilderBuild -> rustTemplate(
                """sleep_impl: self.sleep_impl,""",
                *codegenScope
            )
        }
    }
}

class PubUseAsyncSleep(private val runtimeConfig: RuntimeConfig) : LibRsCustomization() {
    override fun section(section: LibRsSection): Writable {
        return when (section) {
            is LibRsSection.Body -> writable { rust("pub use #T::AsyncSleep;", smithyAsyncRtSleep(runtimeConfig)) }
            else -> emptySection
        }
    }
}

// Generate path to the root module in aws_smithy_async
fun smithyAsyncRtSleep(runtimeConfig: RuntimeConfig) =
    RuntimeType("sleep", runtimeConfig.runtimeCrate("async"), "aws_smithy_async::rt")
