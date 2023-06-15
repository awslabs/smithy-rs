/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.generators.config

import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType.Companion.preludeScope
import software.amazon.smithy.rust.codegen.core.smithy.customize.NamedCustomization

/**
 * Add a `token_provider` field to Service config. See below for the resulting generated code.
 */
class IdempotencyTokenProviderCustomization(codegenContext: ClientCodegenContext) : NamedCustomization<ServiceConfig>() {
    private val runtimeConfig = codegenContext.runtimeConfig
    private val runtimeMode = codegenContext.smithyRuntimeMode
    private val codegenScope = arrayOf(
        *preludeScope,
        "default_provider" to RuntimeType.idempotencyToken(runtimeConfig).resolve("default_provider"),
        "IdempotencyTokenProvider" to RuntimeType.idempotencyToken(runtimeConfig).resolve("IdempotencyTokenProvider"),
    )

    override fun section(section: ServiceConfig): Writable {
        return when (section) {
            is ServiceConfig.ConfigStruct -> writable {
                if (runtimeMode.defaultToMiddleware) {
                    rustTemplate("pub (crate) token_provider: #{IdempotencyTokenProvider},", *codegenScope)
                }
            }

            ServiceConfig.ConfigImpl -> writable {
                if (runtimeMode.defaultToOrchestrator) {
                    rustTemplate(
                        """
                        /// Returns a copy of the idempotency token provider.
                        /// If a random token provider was configured,
                        /// a newly-randomized token provider will be returned.
                        pub fn token_provider(&self) -> #{IdempotencyTokenProvider} {
                            self.inner.load::<#{IdempotencyTokenProvider}>().expect("the idempotency provider should be set").clone()
                        }
                        """,
                        *codegenScope,
                    )
                } else {
                    rustTemplate(
                        """
                        /// Returns a copy of the idempotency token provider.
                        /// If a random token provider was configured,
                        /// a newly-randomized token provider will be returned.
                        pub fn token_provider(&self) -> #{IdempotencyTokenProvider} {
                            self.token_provider.clone()
                        }
                        """,
                        *codegenScope,
                    )
                }
            }

            ServiceConfig.BuilderStruct -> writable {
                rustTemplate("token_provider: #{Option}<#{IdempotencyTokenProvider}>,", *codegenScope)
            }

            ServiceConfig.BuilderImpl -> writable {
                rustTemplate(
                    """
                    /// Sets the idempotency token provider to use for service calls that require tokens.
                    pub fn token_provider(mut self, token_provider: impl #{Into}<#{IdempotencyTokenProvider}>) -> Self {
                        self.set_token_provider(#{Some}(token_provider.into()));
                        self
                    }

                    /// Sets the idempotency token provider to use for service calls that require tokens.
                    pub fn set_token_provider(&mut self, token_provider: #{Option}<#{IdempotencyTokenProvider}>) -> &mut Self {
                        self.token_provider = token_provider;
                        self
                    }
                    """,
                    *codegenScope,
                )
            }

            ServiceConfig.BuilderBuild -> writable {
                if (runtimeMode.defaultToOrchestrator) {
                    rustTemplate(
                        "layer.store_put(self.token_provider.unwrap_or_else(#{default_provider}));",
                        *codegenScope,
                    )
                } else {
                    rustTemplate(
                        "token_provider: self.token_provider.unwrap_or_else(#{default_provider}),",
                        *codegenScope,
                    )
                }
            }

            is ServiceConfig.DefaultForTests -> writable {
                rust("""${section.configBuilderRef}.set_token_provider(Some("00000000-0000-4000-8000-000000000000".into()));""")
            }

            else -> writable { }
        }
    }
}
