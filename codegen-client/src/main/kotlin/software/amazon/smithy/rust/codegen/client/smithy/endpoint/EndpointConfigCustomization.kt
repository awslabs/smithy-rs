/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.endpoint

import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.ClientRustModule
import software.amazon.smithy.rust.codegen.client.smithy.generators.config.ConfigCustomization
import software.amazon.smithy.rust.codegen.client.smithy.generators.config.ServiceConfig
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType.Companion.preludeScope

/**
 * Customization which injects an Endpoints 2.0 Endpoint Resolver into the service config struct
 */
internal class EndpointConfigCustomization(
    private val codegenContext: ClientCodegenContext,
    private val typesGenerator: EndpointTypesGenerator,
) :
    ConfigCustomization() {
    private val runtimeConfig = codegenContext.runtimeConfig
    private val moduleUseName = codegenContext.moduleUseName()
    private val runtimeMode = codegenContext.smithyRuntimeMode
    private val types = Types(runtimeConfig)

    override fun section(section: ServiceConfig): Writable {
        return writable {
            val sharedEndpointResolver = "#{SharedEndpointResolver}<#{Params}>"
            val resolverTrait = "#{SmithyResolver}<#{Params}>"
            val codegenScope = arrayOf(
                *preludeScope,
                "DefaultEndpointResolver" to RuntimeType.smithyRuntime(runtimeConfig).resolve("client::orchestrator::endpoints::DefaultEndpointResolver"),
                "DynEndpointResolver" to RuntimeType.smithyRuntimeApi(runtimeConfig).resolve("client::orchestrator::DynEndpointResolver"),
                "SharedEndpointResolver" to types.sharedEndpointResolver,
                "SmithyResolver" to types.resolveEndpoint,
                "Params" to typesGenerator.paramsStruct(),
            )
            when (section) {
                is ServiceConfig.ConfigStruct -> {
                    if (runtimeMode.defaultToMiddleware) {
                        rustTemplate(
                            "pub (crate) endpoint_resolver: $sharedEndpointResolver,",
                            *codegenScope,
                        )
                    }
                }

                is ServiceConfig.ConfigImpl -> {
                    if (runtimeMode.defaultToOrchestrator) {
                        rustTemplate(
                            """
                            /// Returns the endpoint resolver.
                            pub fn endpoint_resolver(&self) -> $sharedEndpointResolver {
                                self.inner.load::<$sharedEndpointResolver>().expect("endpoint resolver should be set").clone()
                            }
                            """,
                            *codegenScope,
                        )
                    } else {
                        rustTemplate(
                            """
                            /// Returns the endpoint resolver.
                            pub fn endpoint_resolver(&self) -> $sharedEndpointResolver {
                                self.endpoint_resolver.clone()
                            }
                            """,
                            *codegenScope,
                        )
                    }
                }

                is ServiceConfig.BuilderStruct -> {
                    if (runtimeMode.defaultToMiddleware) {
                        rustTemplate(
                            "endpoint_resolver: #{Option}<$sharedEndpointResolver>,",
                            *codegenScope,
                        )
                    }
                }

                ServiceConfig.BuilderImpl -> {
                    // if there are no rules, we don't generate a default resolver—we need to also suppress those docs.
                    val defaultResolverDocs = if (typesGenerator.defaultResolver() != null) {
                        val endpointModule = ClientRustModule.endpoint(codegenContext).fullyQualifiedPath()
                            .replace("crate::", "$moduleUseName::")
                        """
                        ///
                        /// When unset, the client will used a generated endpoint resolver based on the endpoint resolution
                        /// rules for `$moduleUseName`.
                        ///
                        /// ## Examples
                        /// ```no_run
                        /// use aws_smithy_http::endpoint;
                        /// use $endpointModule::{Params as EndpointParams, DefaultResolver};
                        /// /// Endpoint resolver which adds a prefix to the generated endpoint
                        /// ##[derive(Debug)]
                        /// struct PrefixResolver {
                        ///     base_resolver: DefaultResolver,
                        ///     prefix: String
                        /// }
                        /// impl endpoint::ResolveEndpoint<EndpointParams> for PrefixResolver {
                        ///   fn resolve_endpoint(&self, params: &EndpointParams) -> endpoint::Result {
                        ///        self.base_resolver
                        ///              .resolve_endpoint(params)
                        ///              .map(|ep|{
                        ///                   let url = ep.url().to_string();
                        ///                   ep.into_builder().url(format!("{}.{}", &self.prefix, url)).build()
                        ///               })
                        ///   }
                        /// }
                        /// let prefix_resolver = PrefixResolver {
                        ///     base_resolver: DefaultResolver::new(),
                        ///     prefix: "subdomain".to_string()
                        /// };
                        /// let config = $moduleUseName::Config::builder().endpoint_resolver(prefix_resolver);
                        /// ```
                        """
                    } else {
                        ""
                    }
                    rustTemplate(
                        """
                        /// Sets the endpoint resolver to use when making requests.
                        $defaultResolverDocs
                        pub fn endpoint_resolver(mut self, endpoint_resolver: impl $resolverTrait + 'static) -> Self {
                            self.set_endpoint_resolver(#{Some}(#{SharedEndpointResolver}::new(endpoint_resolver)));
                            self
                        }

                        /// Sets the endpoint resolver to use when making requests.
                        ///
                        /// When unset, the client will used a generated endpoint resolver based on the endpoint resolution
                        /// rules for `$moduleUseName`.
                        """,
                        *codegenScope,
                    )

                    if (runtimeMode.defaultToOrchestrator) {
                        rustTemplate(
                            """
                            pub fn set_endpoint_resolver(&mut self, endpoint_resolver: #{Option}<$sharedEndpointResolver>) -> &mut Self {
                                self.inner.store_or_unset(endpoint_resolver);
                                self
                            }
                            """,
                            *codegenScope,
                        )
                    } else {
                        rustTemplate(
                            """
                            pub fn set_endpoint_resolver(&mut self, endpoint_resolver: #{Option}<$sharedEndpointResolver>) -> &mut Self {
                                self.endpoint_resolver = endpoint_resolver;
                                self
                            }
                            """,
                            *codegenScope,
                        )
                    }
                }

                ServiceConfig.BuilderBuild -> {
                    val defaultResolver = typesGenerator.defaultResolver()
                    if (defaultResolver != null) {
                        if (runtimeMode.defaultToOrchestrator) {
                            rustTemplate(
                                // TODO(enableNewSmithyRuntimeCleanup): Simplify the endpoint resolvers
                                """
                                let endpoint_resolver = #{DynEndpointResolver}::new(
                                    #{DefaultEndpointResolver}::<#{Params}>::new(
                                        layer.load::<$sharedEndpointResolver>().cloned().unwrap_or_else(||
                                            #{SharedEndpointResolver}::new(#{DefaultResolver}::new())
                                        )
                                    )
                                );
                                layer.set_endpoint_resolver(endpoint_resolver);
                                """,
                                *codegenScope,
                                "DefaultResolver" to defaultResolver,
                            )
                        } else {
                            rustTemplate(
                                """
                                endpoint_resolver: self.endpoint_resolver.unwrap_or_else(||
                                    #{SharedEndpointResolver}::new(#{DefaultResolver}::new())
                                ),
                                """,
                                *codegenScope,
                                "DefaultResolver" to defaultResolver,
                            )
                        }
                    } else {
                        val alwaysFailsResolver =
                            RuntimeType.forInlineFun("MissingResolver", ClientRustModule.endpoint(codegenContext)) {
                                rustTemplate(
                                    """
                                    ##[derive(Debug)]
                                    pub(crate) struct MissingResolver;
                                    impl<T> #{ResolveEndpoint}<T> for MissingResolver {
                                        fn resolve_endpoint(&self, _params: &T) -> #{Result} {
                                            Err(#{ResolveEndpointError}::message("an endpoint resolver must be provided."))
                                        }
                                    }
                                    """,
                                    "ResolveEndpoint" to types.resolveEndpoint,
                                    "ResolveEndpointError" to types.resolveEndpointError,
                                    "Result" to types.smithyHttpEndpointModule.resolve("Result"),
                                )
                            }
                        // To keep this diff under control, rather than `.expect` here, insert a resolver that will
                        // always fail. In the future, this will be changed to an `expect()`
                        if (runtimeMode.defaultToOrchestrator) {
                            rustTemplate(
                                """
                                let endpoint_resolver = #{DynEndpointResolver}::new(
                                    #{DefaultEndpointResolver}::<#{Params}>::new(
                                        layer.load::<$sharedEndpointResolver>().cloned().unwrap_or_else(||
                                            #{SharedEndpointResolver}::new(#{FailingResolver})
                                        ).clone()
                                    )
                                );
                                layer.set_endpoint_resolver(endpoint_resolver);
                                """,
                                *codegenScope,
                                "FailingResolver" to alwaysFailsResolver,
                            )
                        } else {
                            rustTemplate(
                                """
                                endpoint_resolver: self.endpoint_resolver.unwrap_or_else(||#{SharedEndpointResolver}::new(#{FailingResolver})),
                                """,
                                *codegenScope,
                                "FailingResolver" to alwaysFailsResolver,
                            )
                        }
                    }
                }

                is ServiceConfig.OperationConfigOverride -> {
                    if (runtimeMode.defaultToOrchestrator) {
                        rustTemplate(
                            """
                            if let #{Some}(resolver) = layer
                                .load::<$sharedEndpointResolver>()
                                .cloned()
                            {
                                let endpoint_resolver = #{DynEndpointResolver}::new(
                                    #{DefaultEndpointResolver}::<#{Params}>::new(resolver));
                                layer.set_endpoint_resolver(endpoint_resolver);
                            }
                            """,
                            *codegenScope,
                        )
                    }
                }

                else -> emptySection
            }
        }
    }
}
