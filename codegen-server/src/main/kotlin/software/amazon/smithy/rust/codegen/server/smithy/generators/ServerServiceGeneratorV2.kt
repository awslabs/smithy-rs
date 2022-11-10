/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.model.knowledge.TopDownIndex
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.core.rustlang.RustReservedWords
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.asType
import software.amazon.smithy.rust.codegen.core.rustlang.documentShape
import software.amazon.smithy.rust.codegen.core.rustlang.join
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.util.toPascalCase
import software.amazon.smithy.rust.codegen.core.util.toSnakeCase
import software.amazon.smithy.rust.codegen.server.smithy.ServerCargoDependency
import software.amazon.smithy.rust.codegen.server.smithy.generators.protocol.ServerProtocol

class ServerServiceGeneratorV2(
    codegenContext: CodegenContext,
    private val protocol: ServerProtocol,
) {
    private val runtimeConfig = codegenContext.runtimeConfig
    private val smithyHttpServer = ServerCargoDependency.SmithyHttpServer(runtimeConfig).asType()
    private val codegenScope =
        arrayOf(
            "Bytes" to CargoDependency.Bytes.asType(),
            "Http" to CargoDependency.Http.asType(),
            "HttpBody" to CargoDependency.HttpBody.asType(),
            "SmithyHttpServer" to smithyHttpServer,
            "Tower" to CargoDependency.Tower.asType(),
        )
    private val model = codegenContext.model
    private val symbolProvider = codegenContext.symbolProvider

    private val service = codegenContext.serviceShape
    private val serviceName = service.id.name.toPascalCase()
    private val builderName = "${serviceName}Builder"
    private val builderPluginGenericTypeName = "Plugin"
    private val builderBodyGenericTypeName = "Body"

    /** Calculate all `operationShape`s contained within the `ServiceShape`. */
    private val index = TopDownIndex.of(codegenContext.model)
    private val operations = index.getContainedOperations(codegenContext.serviceShape).toSortedSet(compareBy { it.id })

    /** Associate each operation with the corresponding field names in the builder struct. */
    private val builderFieldNames =
        operations.associateWith { RustReservedWords.escapeIfNeeded(symbolProvider.toSymbol(it).name.toSnakeCase()) }
            .toSortedMap(
                compareBy { it.id },
            )

    /** Associate each operation with the name of the corresponding Zero-Sized Type (ZST) struct name. */
    private val operationStructNames = operations.associateWith { symbolProvider.toSymbol(it).name.toPascalCase() }

    /** A `Writable` block of "field: Type" for the builder. */
    private val builderFields =
        builderFieldNames.values.map { name -> "$name: Option<#{SmithyHttpServer}::routing::Route<Body>>" }

    /** The name of the local private module containing the functions that return the request for each operation */
    private val requestSpecsModuleName = "request_specs"

    /** Associate each operation with a function that returns its request spec. */
    private val requestSpecMap: HashMap<OperationShape, Pair<String, Writable>> by lazy {
        val map = HashMap<OperationShape, Pair<String, Writable>>()
        for (operationShape in operations) {
            val operationName = symbolProvider.toSymbol(operationShape).name
            val spec = protocol.serverRouterRequestSpec(
                operationShape,
                operationName,
                serviceName,
                smithyHttpServer.member("routing::request_spec"),
            )
            val functionName = RustReservedWords.escapeIfNeeded(operationName.toSnakeCase())
            val functionBody = writable {
                rustTemplate(
                    """
                    fn $functionName() -> #{SpecType} {
                        #{Spec:W}
                    }
                    """,
                    "Spec" to spec,
                    "SpecType" to protocol.serverRouterRequestSpecType(smithyHttpServer.member("routing::request_spec")),
                )
            }
            map[operationShape] = Pair(functionName, functionBody)
        }
        map
    }

    /** A `Writable` block containing all the `Handler` and `Operation` setters for the builder. */
    private fun builderSetters(): Writable = writable {
        for ((operationShape, structName) in operationStructNames) {
            val fieldName = builderFieldNames[operationShape]
            rustTemplate(
                """
                /// Sets the [`$structName`](crate::operation_shape::$structName) operation.
                ///
                /// This should be an async function satisfying the [`Handler`](#{SmithyHttpServer}::operation::Handler) trait.
                /// See the [operation module documentation](#{SmithyHttpServer}::operation) for more information.
                pub fn $fieldName<HandlerType, Extensions>(self, handler: HandlerType) -> Self
                where
                    HandlerType: #{SmithyHttpServer}::operation::Handler<crate::operation_shape::$structName, Extensions>,
                    #{SmithyHttpServer}::operation::Operation<#{SmithyHttpServer}::operation::IntoService<crate::operation_shape::$structName, HandlerType>>:
                        #{SmithyHttpServer}::operation::Upgradable<
                            #{Protocol},
                            crate::operation_shape::$structName,
                            Extensions,
                            $builderBodyGenericTypeName,
                            $builderPluginGenericTypeName,
                        >
                {
                    use #{SmithyHttpServer}::operation::OperationShapeExt;
                    self.${fieldName}_operation(crate::operation_shape::$structName::from_handler(handler))
                }

                /// Sets the [`$structName`](crate::operation_shape::$structName) operation.
                ///
                /// This should be an [`Operation`](#{SmithyHttpServer}::operation::Operation) created from
                /// [`$structName`](crate::operation_shape::$structName) using either
                /// [`OperationShape::from_handler`](#{SmithyHttpServer}::operation::OperationShapeExt::from_handler) or
                /// [`OperationShape::from_service`](#{SmithyHttpServer}::operation::OperationShapeExt::from_service).
                pub fn ${fieldName}_operation<Operation, Extensions>(mut self, operation: Operation) -> Self
                where
                    Operation: #{SmithyHttpServer}::operation::Upgradable<
                        #{Protocol},
                        crate::operation_shape::$structName,
                        Extensions,
                        $builderBodyGenericTypeName,
                        $builderPluginGenericTypeName,
                    >
                {
                    self.$fieldName = Some(operation.upgrade(&self.plugin));
                    self
                }
                """,
                "Protocol" to protocol.markerStruct(),
                *codegenScope,
            )

            // Adds newline between setters.
            rust("")
        }
    }

    private fun buildMethod(): Writable = writable {
        val missingOperationsVariableName = "missing_operation_names"

        val nullabilityChecks = writable {
            for (operationShape in operations) {
                val fieldName = builderFieldNames[operationShape]!!
                val operationZstTypeName = operationStructNames[operationShape]!!
                rust(
                    """
                    if self.$fieldName.is_none() {
                        $missingOperationsVariableName.insert(crate::operation_shape::$operationZstTypeName::NAME, ".$fieldName()");
                    }
                    """,
                )
            }
        }
        val routesArrayElements = writable {
            for (operationShape in operations) {
                val fieldName = builderFieldNames[operationShape]!!
                val (specBuilderFunctionName, _) = requestSpecMap.getValue(operationShape)
                rust(
                    """
                    ($requestSpecsModuleName::$specBuilderFunctionName(), self.$fieldName.expect("this should never panic since we are supposed to check beforehand that a handler has been registered for this operation; please file a bug report under https://github.com/awslabs/smithy-rs/issues")),
                    """,
                )
            }
        }
        rustTemplate(
            """
            /// Constructs a [`$serviceName`] from the arguments provided to the builder.
            ///
            /// Forgetting to register a handler for one or more operations will result in an error.
            pub fn build(self) -> Result<$serviceName<#{SmithyHttpServer}::routing::Route<$builderBodyGenericTypeName>>, MissingOperationsError>
            {
                let router = {
                    use #{SmithyHttpServer}::operation::OperationShape;
                    let mut $missingOperationsVariableName = std::collections::HashMap::new();
                    #{NullabilityChecks:W}
                    if !$missingOperationsVariableName.is_empty() {
                        return Err(MissingOperationsError {
                            operation_names2setter_methods: $missingOperationsVariableName,
                        });
                    }
                    #{Router}::from_iter([#{RoutesArrayElements:W}])
                };
                Ok($serviceName {
                    router: #{SmithyHttpServer}::routers::RoutingService::new(router),
                })
            }
            """,
            "Router" to protocol.routerType(),
            "NullabilityChecks" to nullabilityChecks,
            "RoutesArrayElements" to routesArrayElements,
            "SmithyHttpServer" to smithyHttpServer,
        )
    }

    private fun buildUncheckedMethod(): Writable = writable {
        val pairs = writable {
            for (operationShape in operations) {
                val fieldName = builderFieldNames[operationShape]!!
                val (specBuilderFunctionName, _) = requestSpecMap.getValue(operationShape)
                val operationZstTypeName = operationStructNames[operationShape]!!
                rustTemplate(
                    """
                    (
                        $requestSpecsModuleName::$specBuilderFunctionName(),
                        self.$fieldName.unwrap_or_else(|| {
                            #{SmithyHttpServer}::routing::Route::new(<#{SmithyHttpServer}::operation::FailOnMissingOperation as #{SmithyHttpServer}::operation::Upgradable<
                                #{Protocol},
                                crate::operation_shape::$operationZstTypeName,
                                (),
                                _,
                                _,
                            >>::upgrade(#{SmithyHttpServer}::operation::FailOnMissingOperation, &self.plugin))
                        })
                    ),
                    """,
                    "SmithyHttpServer" to smithyHttpServer,
                    "Protocol" to protocol.markerStruct(),
                )
            }
        }
        rustTemplate(
            """
            /// Constructs a [`$serviceName`] from the arguments provided to the builder.
            /// Operations without a handler default to returning 500s to the caller.
            ///
            /// Check out [`$builderName::build`] if you'd prefer the builder to fail if one or more operations do
            /// not have a registered handler.
            pub fn build_unchecked(self) -> $serviceName<#{SmithyHttpServer}::routing::Route<$builderBodyGenericTypeName>>
            where
                $builderBodyGenericTypeName: Send + 'static
            {
                let router = #{Router}::from_iter([#{Pairs:W}]);
                $serviceName {
                    router: #{SmithyHttpServer}::routers::RoutingService::new(router),
                }
            }
            """,
            "Router" to protocol.routerType(),
            "Pairs" to pairs,
            "SmithyHttpServer" to smithyHttpServer,
        )
    }

    /** Returns a `Writable` containing the builder struct definition and its implementations. */
    private fun builder(): Writable = writable {
        val builderGenerics = listOf(builderBodyGenericTypeName, builderPluginGenericTypeName).joinToString(", ")
        rustTemplate(
            """
            /// The service builder for [`$serviceName`].
            ///
            /// Constructed via [`$serviceName::builder_with_plugins`] or [`$serviceName::builder_without_plugins`].
            pub struct $builderName<$builderGenerics> {
                ${builderFields.joinToString(", ")},
                plugin: $builderPluginGenericTypeName,
            }

            impl<$builderGenerics> $builderName<$builderGenerics> {
                #{Setters:W}
            }

            impl<$builderGenerics> $builderName<$builderGenerics> {
                #{BuildMethod:W}

                #{BuildUncheckedMethod:W}
            }
            """,
            "Setters" to builderSetters(),
            "BuildMethod" to buildMethod(),
            "BuildUncheckedMethod" to buildUncheckedMethod(),
            *codegenScope,
        )
    }

    private fun requestSpecsModule(): Writable = writable {
        val functions = writable {
            for ((_, function) in requestSpecMap.values) {
                rustTemplate(
                    """
                    pub(super) #{Function:W}
                    """,
                    "Function" to function,
                )
            }
        }
        rustTemplate(
            """
            mod $requestSpecsModuleName {
                #{SpecFunctions:W}
            }
            """,
            "SpecFunctions" to functions,
        )
    }

    /** Returns a `Writable` comma delimited sequence of `builder_field: None`. */
    private val notSetFields = builderFieldNames.values.map {
        writable {
            rustTemplate(
                "$it: None",
                *codegenScope,
            )
        }
    }

    /** Returns a `Writable` containing the service struct definition and its implementations. */
    private fun serviceStruct(): Writable = writable {
        documentShape(service, model)

        rustTemplate(
            """
            ##[derive(Clone)]
            pub struct $serviceName<S = #{SmithyHttpServer}::routing::Route> {
                router: #{SmithyHttpServer}::routers::RoutingService<#{Router}<S>, #{Protocol}>,
            }

            impl $serviceName<()> {
                /// Constructs a builder for [`$serviceName`].
                /// You must specify what plugins should be applied to the operations in this service.
                ///
                /// Use [`$serviceName::builder_without_plugins`] if you don't need to apply plugins.
                pub fn builder_with_plugins<Body, Plugin>(plugin: Plugin) -> $builderName<Body, Plugin> {
                    $builderName {
                        #{NotSetFields:W},
                        plugin
                    }
                }

                /// Constructs a builder for [`$serviceName`].
                ///
                /// Use [`$serviceName::builder_without_plugins`] if you need to specify plugins.
                pub fn builder_without_plugins<Body>() -> $builderName<Body, #{SmithyHttpServer}::plugin::IdentityPlugin> {
                    Self::builder_with_plugins(#{SmithyHttpServer}::plugin::IdentityPlugin)
                }
            }

            impl<S> $serviceName<S> {
                /// Converts [`$serviceName`] into a [`MakeService`](tower::make::MakeService).
                pub fn into_make_service(self) -> #{SmithyHttpServer}::routing::IntoMakeService<Self> {
                    #{SmithyHttpServer}::routing::IntoMakeService::new(self)
                }

                /// Applies a [`Layer`](#{Tower}::Layer) uniformly to all routes.
                pub fn layer<L>(self, layer: &L) -> $serviceName<L::Service>
                where
                    L: #{Tower}::Layer<S>
                {
                    $serviceName {
                        router: self.router.map(|s| s.layer(layer))
                    }
                }

                /// Applies [`Route::new`](#{SmithyHttpServer}::routing::Route::new) to all routes.
                ///
                /// This has the effect of erasing all types accumulated via [`layer`].
                pub fn boxed<B>(self) -> $serviceName<#{SmithyHttpServer}::routing::Route<B>>
                where
                    S: #{Tower}::Service<
                        #{Http}::Request<B>,
                        Response = #{Http}::Response<#{SmithyHttpServer}::body::BoxBody>,
                        Error = std::convert::Infallible>,
                    S: Clone + Send + 'static,
                    S::Future: Send + 'static,
                {
                    self.layer(&#{Tower}::layer::layer_fn(#{SmithyHttpServer}::routing::Route::new))
                }
            }

            impl<B, RespB, S> #{Tower}::Service<#{Http}::Request<B>> for $serviceName<S>
            where
                S: #{Tower}::Service<#{Http}::Request<B>, Response = #{Http}::Response<RespB>> + Clone,
                RespB: #{HttpBody}::Body<Data = #{Bytes}::Bytes> + Send + 'static,
                RespB::Error: Into<Box<dyn std::error::Error + Send + Sync>>
            {
                type Response = #{Http}::Response<#{SmithyHttpServer}::body::BoxBody>;
                type Error = S::Error;
                type Future = #{SmithyHttpServer}::routers::RoutingFuture<S, B>;

                fn poll_ready(&mut self, cx: &mut std::task::Context) -> std::task::Poll<Result<(), Self::Error>> {
                    self.router.poll_ready(cx)
                }

                fn call(&mut self, request: #{Http}::Request<B>) -> Self::Future {
                    self.router.call(request)
                }
            }
            """,
            "NotSetFields" to notSetFields.join(", "),
            "Router" to protocol.routerType(),
            "Protocol" to protocol.markerStruct(),
            *codegenScope,
        )
    }

    private fun missingOperationsError(): Writable = writable {
        rust(
            """
            ##[derive(Debug)]
            pub struct MissingOperationsError {
                operation_names2setter_methods: std::collections::HashMap<&'static str, &'static str>,
            }

            impl std::fmt::Display for MissingOperationsError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(
                        f,
                        "You must specify a handler for all operations attached to `$serviceName`.\n\
                        We are missing handlers for the following operations:\n",
                    )?;
                    for operation_name in self.operation_names2setter_methods.keys() {
                        writeln!(f, "- {}", operation_name)?;
                    }

                    writeln!(f, "\nUse the dedicated methods on `$builderName` to register the missing handlers:")?;
                    for setter_name in self.operation_names2setter_methods.values() {
                        writeln!(f, "- {}", setter_name)?;
                    }
                    Ok(())
                }
            }

            impl std::error::Error for MissingOperationsError {}
            """,
        )
    }

    fun render(writer: RustWriter) {
        writer.rustTemplate(
            """
            #{Builder:W}

            #{MissingOperationsError:W}

            #{RequestSpecs:W}

            #{Struct:W}
            """,
            "Builder" to builder(),
            "MissingOperationsError" to missingOperationsError(),
            "RequestSpecs" to requestSpecsModule(),
            "Struct" to serviceStruct(),
        )
    }
}
