/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.generators

import software.amazon.smithy.model.knowledge.TopDownIndex
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.rust.codegen.rustlang.Attribute
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.Feature
import software.amazon.smithy.rust.codegen.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.rustlang.RustModule
import software.amazon.smithy.rust.codegen.rustlang.RustType
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.contains
import software.amazon.smithy.rust.codegen.rustlang.documentShape
import software.amazon.smithy.rust.codegen.rustlang.render
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.stripOuter
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.RustCrate
import software.amazon.smithy.rust.codegen.smithy.customize.RustCodegenDecorator
import software.amazon.smithy.rust.codegen.smithy.generators.error.errorSymbol
import software.amazon.smithy.rust.codegen.smithy.rustType
import software.amazon.smithy.rust.codegen.util.inputShape
import software.amazon.smithy.rust.codegen.util.outputShape
import software.amazon.smithy.rust.codegen.util.toSnakeCase

class FluentClientDecorator : RustCodegenDecorator {
    override val name: String = "FluentClient"
    override val order: Byte = 0

    private fun applies(protocolConfig: ProtocolConfig): Boolean = protocolConfig.symbolProvider.config().codegenConfig.includeFluentClient

    override fun extras(protocolConfig: ProtocolConfig, rustCrate: RustCrate) {
        if (!applies(protocolConfig)) {
            return
        }

        val module = RustMetadata(additionalAttributes = listOf(Attribute.Cfg.feature("client")), public = true)
        rustCrate.withModule(RustModule("client", module)) { writer ->
            FluentClientGenerator(protocolConfig).render(writer)
        }
        val smithyClient = CargoDependency.SmithyClient(protocolConfig.runtimeConfig)
        rustCrate.addFeature(Feature("client", true, listOf(smithyClient.name)))
        rustCrate.addFeature(Feature("rustls", default = true, listOf("smithy-client/rustls")))
        rustCrate.addFeature(Feature("native-tls", default = false, listOf("smithy-client/native-tls")))
    }

    override fun libRsCustomizations(
        protocolConfig: ProtocolConfig,
        baseCustomizations: List<LibRsCustomization>
    ): List<LibRsCustomization> {
        if (!applies(protocolConfig)) {
            return baseCustomizations
        }

        return baseCustomizations + object : LibRsCustomization() {
            override fun section(section: LibRsSection) = when (section) {
                is LibRsSection.Body -> writable {
                    Attribute.Cfg.feature("client").render(this)
                    rust("pub use client::{Client, Builder};")
                }
                else -> emptySection
            }
        }
    }
}

class FluentClientGenerator(protocolConfig: ProtocolConfig) {
    private val serviceShape = protocolConfig.serviceShape
    private val operations =
        TopDownIndex.of(protocolConfig.model).getContainedOperations(serviceShape).sortedBy { it.id }
    private val symbolProvider = protocolConfig.symbolProvider
    private val model = protocolConfig.model
    private val clientDep = CargoDependency.SmithyClient(protocolConfig.runtimeConfig).copy(optional = true)
    private val runtimeConfig = protocolConfig.runtimeConfig
    private val moduleName = protocolConfig.moduleName
    private val moduleUseName = moduleName.replace("-", "_")
    private val humanName = serviceShape.id.name

    fun render(writer: RustWriter) {
        writer.rustTemplate(
            """
            ##[derive(Debug)]
            pub(crate) struct Handle<C, M, R> {
                client: #{client}::Client<C, M, R>,
                conf: crate::Config,
            }

            /// An ergonomic service client for `$humanName`.
            ///
            /// This client allows ergonomic access to a `$humanName`-shaped service.
            /// Each method corresponds to an endpoint defined in the service's Smithy model,
            /// and the request and response shapes are auto-generated from that same model.
            ///
            /// ## Constructing a Client
            ///
            /// To construct a client, you need a few different things:
            ///
            /// - A [`Config`](crate::Config) that specifies additional configuration
            ///   required by the service.
            /// - A connector (`C`) that specifies how HTTP requests are translated
            ///   into HTTP responses. This will typically be an HTTP client (like
            ///   `hyper`), though you can also substitute in your own, like a mock
            ///   mock connector for testing.
            /// - A "middleware" (`M`) that modifies requests prior to them being
            ///   sent to the request. Most commonly, middleware will decide what
            ///   endpoint the requests should be sent to, as well as perform
            ///   authentcation and authorization of requests (such as SigV4).
            ///   You can also have middleware that performs request/response
            ///   tracing, throttling, or other middleware-like tasks.
            /// - A retry policy (`R`) that dictates the behavior for requests that
            ///   fail and should (potentially) be retried. The default type is
            ///   generally what you want, as it implements a well-vetted retry
            ///   policy described in TODO.
            ///
            /// To construct a client, you will generally want to call
            /// [`Client::with_config`], which takes a [`#{client}::Client`] (a
            /// Smithy client that isn't specialized to a particular service),
            /// and a [`Config`](crate::Config). Both of these are constructed using
            /// the [builder pattern] where you first construct a `Builder` type,
            /// then configure it with the necessary parameters, and then call
            /// `build` to construct the finalized output type. The
            /// [`#{client}::Client`] builder is re-exported in this crate as
            /// [`Builder`] for convenience.
            ///
            /// In _most_ circumstances, you will want to use the following pattern
            /// to construct a client:
            ///
            /// ```
            /// use $moduleUseName::{Builder, Client, Config};
            /// let raw_client =
            ///     Builder::new()
            ///       .https()
            /// ##     /*
            ///       .middleware(/* discussed below */)
            /// ##     */
            /// ##     .middleware_fn(|r| r)
            ///       .build();
            /// let config = Config::builder().build();
            /// let client = Client::with_config(raw_client, config);
            /// ```
            ///
            /// For the middleware, you'll want to use whatever matches the
            /// routing, authentication and authorization required by the target
            /// service. For example, for the standard AWS SDK which uses
            /// [SigV4-signed requests], the middleware looks like this:
            ///
            // Ignored as otherwise we'd need to pull in all these dev-dependencies.
            /// ```rust,ignore
            /// use aws_endpoint::AwsEndpointStage;
            /// use aws_http::user_agent::UserAgentStage;
            /// use aws_sig_auth::middleware::SigV4SigningStage;
            /// use aws_sig_auth::signer::SigV4Signer;
            /// use smithy_http_tower::map_request::MapRequestLayer;
            /// use tower::layer::util::Stack;
            /// use tower::ServiceBuilder;
            ///
            /// type AwsMiddlewareStack =
            ///     Stack<MapRequestLayer<SigV4SigningStage>,
            ///         Stack<MapRequestLayer<UserAgentStage>,
            ///             MapRequestLayer<AwsEndpointStage>>>,
            ///
            /// ##[derive(Debug, Default)]
            /// pub struct AwsMiddleware;
            /// impl<S> tower::Layer<S> for AwsMiddleware {
            ///     type Service = <AwsMiddlewareStack as tower::Layer<S>>::Service;
            ///
            ///     fn layer(&self, inner: S) -> Self::Service {
            ///         let signer = MapRequestLayer::for_mapper(SigV4SigningStage::new(SigV4Signer::new())); _signer: MapRequestLaye
            ///         let endpoint_resolver = MapRequestLayer::for_mapper(AwsEndpointStage); _endpoint_resolver: MapRequestLayer<Aw
            ///         let user_agent = MapRequestLayer::for_mapper(UserAgentStage::new()); _user_agent: MapRequestLayer<UserAgentSt
            ///         // These layers can be considered as occuring in order, that is:
            ///         // 1. Resolve an endpoint
            ///         // 2. Add a user agent
            ///         // 3. Sign
            ///         // (4. Dispatch over the wire)
            ///         ServiceBuilder::new() _ServiceBuilder<Identity>
            ///             .layer(endpoint_resolver) _ServiceBuilder<Stack<MapRequestLayer<_>, _>>
            ///             .layer(user_agent) _ServiceBuilder<Stack<MapRequestLayer<_>, _>>
            ///             .layer(signer) _ServiceBuilder<Stack<MapRequestLayer<_>, _>>
            ///             .service(inner)
            ///     }
            /// }
            /// ```
            ///
            /// ## Using a Client
            ///
            /// Once you have a client set up, you can access the service's endpoints
            /// by calling the appropriate method on [`Client`]. Each such method
            /// returns a request builder for that endpoint, with methods for setting
            /// the various fields of the request. Once your request is complete, use
            /// the `send` method to send the request. `send` returns a future, which
            /// you then have to `.await` to get the service's response.
            ///
            /// [builder pattern]: https://rust-lang.github.io/api-guidelines/type-safety.html##c-builder
            /// [SigV4-signed requests]: https://docs.aws.amazon.com/general/latest/gr/signature-version-4.html
            ##[derive(Clone, std::fmt::Debug)]
            pub struct Client<C, M, R = #{client}::retry::Standard> {
                // TODO: Why Arc<>?
                handle: std::sync::Arc<Handle<C, M, R>>
            }

            ##[doc(inline)]
            pub use #{client}::Builder;

            impl<C, M, R> From<#{client}::Client<C, M, R>> for Client<C, M, R> {
                fn from(client: #{client}::Client<C, M, R>) -> Self {
                    Self::with_config(client, crate::Config::builder().build())
                }
            }

            impl<C, M, R> Client<C, M, R> {
                pub fn with_config(client: #{client}::Client<C, M, R>, conf: crate::Config) -> Self {
                    Self {
                        handle: std::sync::Arc::new(Handle {
                            client,
                            conf,
                        })
                    }
                }

                pub fn conf(&self) -> &crate::Config {
                    &self.handle.conf
                }
            }
        """,
            "client" to clientDep.asType()
        )
        writer.rustBlockTemplate(
            """
            impl<C, M, R> Client<C, M, R>
              where
                C: #{client}::bounds::SmithyConnector,
                M: #{client}::bounds::SmithyMiddleware<C>,
                R: #{client}::retry::NewRequestPolicy,
            """,
            "client" to clientDep.asType(),
        ) {
            operations.forEach { operation ->
                val name = symbolProvider.toSymbol(operation).name
                rust(
                    """
                    pub fn ${name.toSnakeCase()}(&self) -> fluent_builders::$name<C, M, R> {
                        fluent_builders::$name::new(self.handle.clone())
                    }"""
                )
            }
        }
        writer.withModule("fluent_builders") {
            operations.forEach { operation ->
                val name = symbolProvider.toSymbol(operation).name
                val input = operation.inputShape(model)
                val members: List<MemberShape> = input.allMembers.values.toList()

                rust(
                    """
                ##[derive(std::fmt::Debug)]
                pub struct $name<C, M, R> {
                    handle: std::sync::Arc<super::Handle<C, M, R>>,
                    inner: #T
                }""",
                    input.builderSymbol(symbolProvider)
                )

                rustBlockTemplate(
                    """
                    impl<C, M, R> $name<C, M, R>
                      where
                        C: #{client}::bounds::SmithyConnector,
                        M: #{client}::bounds::SmithyMiddleware<C>,
                        R: #{client}::retry::NewRequestPolicy,
                    """,
                    "client" to CargoDependency.SmithyClient(runtimeConfig).asType(),
                ) {
                    rustTemplate(
                        """
                    pub(crate) fn new(handle: std::sync::Arc<super::Handle<C, M, R>>) -> Self {
                        Self { handle, inner: Default::default() }
                    }

                    pub async fn send(self) -> std::result::Result<#{ok}, #{sdk_err}<#{operation_err}>> where
                        R::Policy: #{client}::bounds::SmithyRetryPolicy<#{input}OperationOutputAlias, #{ok}, #{operation_err}, #{input}OperationRetryAlias>,
                    {
                        let input = self.inner.build().map_err(|err|#{sdk_err}::ConstructionFailure(err.into()))?;
                        let op = input.make_operation(&self.handle.conf)
                            .map_err(|err|#{sdk_err}::ConstructionFailure(err.into()))?;
                        self.handle.client.call(op).await
                    }
                    """,
                        "input" to symbolProvider.toSymbol(operation.inputShape(model)),
                        "ok" to symbolProvider.toSymbol(operation.outputShape(model)),
                        "operation_err" to operation.errorSymbol(symbolProvider),
                        "sdk_err" to CargoDependency.SmithyHttp(runtimeConfig).asType().copy(name = "result::SdkError"),
                        "client" to CargoDependency.SmithyClient(runtimeConfig).asType(),
                    )
                    members.forEach { member ->
                        val memberName = symbolProvider.toMemberName(member)
                        // All fields in the builder are optional
                        val memberSymbol = symbolProvider.toSymbol(member)
                        val outerType = memberSymbol.rustType()
                        val coreType = outerType.stripOuter<RustType.Option>()
                        when (coreType) {
                            is RustType.Vec -> renderVecHelper(member, memberName, coreType)
                            is RustType.HashMap -> renderMapHelper(member, memberName, coreType)
                            else -> {
                                val signature = when (coreType) {
                                    is RustType.String,
                                    is RustType.Box -> "(mut self, inp: impl Into<${coreType.render(true)}>) -> Self"
                                    else -> "(mut self, inp: ${coreType.render(true)}) -> Self"
                                }
                                documentShape(member, model)
                                rustBlock("pub fn $memberName$signature") {
                                    write("self.inner = self.inner.$memberName(inp);")
                                    write("self")
                                }
                            }
                        }
                        // pure setter
                        rustBlock("pub fn ${member.setterName()}(mut self, inp: ${outerType.render(true)}) -> Self") {
                            rust(
                                """
                                self.inner = self.inner.${member.setterName()}(inp);
                                self
                                """
                            )
                        }
                    }
                }
            }
        }
    }

    private fun RustWriter.renderMapHelper(member: MemberShape, memberName: String, coreType: RustType.HashMap) {
        documentShape(member, model)
        val k = coreType.key
        val v = coreType.member

        rustBlock("pub fn $memberName(mut self, k: impl Into<${k.render()}>, v: impl Into<${v.render()}>) -> Self") {
            rust(
                """
                self.inner = self.inner.$memberName(k, v);
                self
            """
            )
        }
    }

    private fun RustWriter.renderVecHelper(member: MemberShape, memberName: String, coreType: RustType.Vec) {
        documentShape(member, model)
        rustBlock("pub fn $memberName(mut self, inp: impl Into<${coreType.member.render(true)}>) -> Self") {
            rust(
                """
                self.inner = self.inner.$memberName(inp);
                self
            """
            )
        }
    }
}
