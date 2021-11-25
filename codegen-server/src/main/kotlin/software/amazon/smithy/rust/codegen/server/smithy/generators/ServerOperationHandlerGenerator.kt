/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.server.smithy.ServerCargoDependency
import software.amazon.smithy.rust.codegen.server.smithy.protocols.ServerHttpProtocolGenerator
import software.amazon.smithy.rust.codegen.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.generators.error.errorSymbol
import software.amazon.smithy.rust.codegen.util.outputShape

/**
 * ServerOperationHandlerGenerator
 */
class ServerOperationHandlerGenerator(
    codegenContext: CodegenContext,
    private val operations: List<OperationShape>,
) {
    private val serverCrate = "aws_smithy_http_server"
    private val service = codegenContext.serviceShape
    private val model = codegenContext.model
    private val symbolProvider = codegenContext.symbolProvider
    private val operationNames = operations.map { symbolProvider.toSymbol(it).name }
    private val runtimeConfig = codegenContext.runtimeConfig
    private val codegenScope = arrayOf(
        "Axum" to ServerCargoDependency.Axum.asType(),
        "PinProject" to ServerCargoDependency.PinProject.asType(),
        "Tower" to ServerCargoDependency.Tower.asType(),
        "FuturesUtil" to ServerCargoDependency.FuturesUtil.asType(),
        "SmithyHttpServer" to CargoDependency.SmithyHttpServer(runtimeConfig).asType(),
        "SmithyRejection" to ServerHttpProtocolGenerator.smithyRejection(runtimeConfig),
        "http" to RuntimeType.http,
    )

    fun render(writer: RustWriter) {
        renderStaticRust(writer)
        renderHandlersImpl(writer)
        renderHandlersImplWithState(writer)
    }

    private fun renderHandlersImpl(writer: RustWriter) {
        operations.map { operation ->
            val operationName = symbolProvider.toSymbol(operation).name
            val inputName = "crate::input::${operationName}Input"
            val inputWrapperName = "crate::operation::$operationName${ServerHttpProtocolGenerator.OPERATION_INPUT_WRAPPER_SUFFIX}"
            val outputWrapperName = "crate::operation::$operationName${ServerHttpProtocolGenerator.OPERATION_OUTPUT_WRAPPER_SUFFIX}"
            writer.rustTemplate(
                """
                ##[axum::async_trait]
                impl<B, Fun, Fut> Handler<B, (), $inputName> for Fun
                where
                    ${operationTraitBounds(operation, operationName, false)}
                {
                    type Sealed = sealed::Hidden;

                    async fn call(self, req: #{http}::Request<B>) -> #{http}::Response<#{SmithyHttpServer}::BoxBody> {
                        let mut req = #{Axum}::extract::RequestParts::new(req);
                        use #{Axum}::extract::FromRequest;
                        use #{Axum}::response::IntoResponse;
                        let input_wrapper = match $inputWrapperName::from_request(&mut req).await {
                            Ok(v) => v,
                            Err(r) => return r.into_response().map(#{SmithyHttpServer}::body::box_body)
                        };
                        let input_inner = input_wrapper.into();
                        let output_inner = self(input_inner).await;
                        let output_wrapper: $outputWrapperName = output_inner.into();
                        output_wrapper.into_response().map(#{SmithyHttpServer}::body::box_body)
                    }
                }
                """,
                *codegenScope
            )
        }
    }

    private fun renderHandlersImplWithState(writer: RustWriter) {
        operations.map { operation ->
            val operationName = symbolProvider.toSymbol(operation).name
            val inputName = "crate::input::${operationName}Input"
            val inputWrapperName = "crate::operation::$operationName${ServerHttpProtocolGenerator.OPERATION_INPUT_WRAPPER_SUFFIX}"
            val outputWrapperName = "crate::operation::$operationName${ServerHttpProtocolGenerator.OPERATION_OUTPUT_WRAPPER_SUFFIX}"
            writer.rustTemplate(
                """
                ##[axum::async_trait]
                impl<B, Fun, Fut, S> Handler<B, #{SmithyHttpServer}::Extension<S>, $inputName> for Fun
                where
                    ${operationTraitBounds(operation, operationName, true)}
                {
                    type Sealed = sealed::Hidden;

                    async fn call(self, req: #{http}::Request<B>) -> #{http}::Response<#{SmithyHttpServer}::BoxBody> {
                        let mut req = #{Axum}::extract::RequestParts::new(req);
                        use #{Axum}::extract::FromRequest;
                        use #{Axum}::response::IntoResponse;
                        let input_wrapper = match $inputWrapperName::from_request(&mut req).await {
                            Ok(v) => v,
                            Err(r) => return r.into_response().map(#{SmithyHttpServer}::body::box_body)
                        };
                        let state = match #{SmithyHttpServer}::Extension::<S>::from_request(&mut req).await {
                            Ok(v) => v,
                            Err(r) => return r.into_response().map(#{SmithyHttpServer}::body::box_body)
                        };
                        let input_inner = input_wrapper.into();
                        let output_inner = self(input_inner, state).await;
                        let output_wrapper: $outputWrapperName = output_inner.into();
                        output_wrapper.into_response().map(#{SmithyHttpServer}::body::box_body)
                    }
                }
                """,
                *codegenScope
            )
        }
    }

    private fun operationTraitBounds(operation: OperationShape, operationName: String, state: Boolean): String {
        val inputName = "crate::input::${operationName}Input"
        val inputFn = if (state) {
            """S: Send + Clone + Sync + 'static,
            Fun: FnOnce($inputName, $serverCrate::Extension<S>) -> Fut + Clone + Send + 'static,"""
        } else {
            "Fun: FnOnce($inputName) -> Fut + Clone + Send + 'static,"
        }
        val outputType = if (operation.errors.isNotEmpty()) {
            "Result<${symbolProvider.toSymbol(operation.outputShape(model)).fullName}, ${operation.errorSymbol(symbolProvider).fullyQualifiedName()}>"
        } else {
            symbolProvider.toSymbol(operation.outputShape(model)).fullName
        }
        return """
            $inputFn
            Fut: std::future::Future<Output = $outputType> + Send,
            B: $serverCrate::HttpBody + Send + 'static,
            B::Data: Send,
            B::Error: Into<$serverCrate::BoxError>,
            $serverCrate::rejection::SmithyRejection: From<<B as $serverCrate::HttpBody>::Error>
        """
    }

    private fun renderStaticRust(writer: RustWriter) {
        writer.rustTemplate(
            """
            use aws_smithy_http_server::body::{box_body, BoxBody};
            use aws_smithy_http_server::{opaque_future, Extension};
            use #{FuturesUtil}::{
                future::{BoxFuture, Map},
                FutureExt,
            };
            use http::{Request, Response};
            use #{PinProject};
            use std::{
                marker::PhantomData,
                convert::Infallible,
                task::{Context, Poll},
            };
            use #{Tower}::Service;
            /// Struct that holds a handler, that is, a function provided by the user that implements the
            /// Smithy operation.
            pub struct OperationHandler<H, B, R, I> {
                handler: H,
                ##[allow(clippy::type_complexity)]
                _marker: PhantomData<fn() -> (B, R, I)>,
            }
            impl<H, B, R, I> Clone for OperationHandler<H, B, R, I>
            where
                H: Clone,
            {
                fn clone(&self) -> Self {
                    Self {
                        handler: self.handler.clone(),
                        _marker: PhantomData,
                    }
                }
            }
            /// Construct an [`OperationHandler`] out of a function implementing the operation.
            pub fn operation<H, B, R, I>(handler: H) -> OperationHandler<H, B, R, I> {
                OperationHandler {
                    handler,
                    _marker: PhantomData,
                }
            }
            impl<H, B, R, I> Service<Request<B>> for OperationHandler<H, B, R, I>
            where
                H: Handler<B, R, I>,
                B: Send + 'static,
            {
                type Response = Response<BoxBody>;
                type Error = Infallible;
                type Future = OperationHandlerFuture;

                ##[inline]
                fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                    Poll::Ready(Ok(()))
                }

                fn call(&mut self, req: Request<B>) -> Self::Future {
                    let future = Handler::call(self.handler.clone(), req).map(Ok::<_, Infallible> as _);
                    OperationHandlerFuture::new(future)
                }
            }
            type WrapResultInResponseFn = fn(Response<BoxBody>) -> Result<Response<BoxBody>, Infallible>;
            opaque_future! {
                /// Response future for [`OperationHandler`].
                pub type OperationHandlerFuture =
                    Map<BoxFuture<'static, Response<BoxBody>>, WrapResultInResponseFn>;
            }
            pub(crate) mod sealed {
                ##![allow(unreachable_pub, missing_docs, missing_debug_implementations)]
                pub trait HiddenTrait {}
                pub struct Hidden;
                impl HiddenTrait for Hidden {}
            }
            ##[axum::async_trait]
            pub trait Handler<B, T, Fut>: Clone + Send + Sized + 'static {
                ##[doc(hidden)]
                type Sealed: sealed::HiddenTrait;

                async fn call(self, req: Request<B>) -> Response<BoxBody>;
            }
            """,
            *codegenScope
        )
    }
}
