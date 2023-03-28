/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! # Request IDs
//!
//! `aws-smithy-http-server` provides the [`ServerRequestId`].
//!
//! ## `ServerRequestId`
//!
//! A [`ServerRequestId`] is an opaque random identifier generated by the server every time it receives a request.
//! It uniquely identifies the request within that service instance. It can be used to collate all logs, events and
//! data related to a single operation.
//! Use [`ServerRequestIdProviderLayer::new`] to use [`ServerRequestId`] in your handler.
//!
//! The [`ServerRequestId`] can be returned to the caller, who can in turn share the [`ServerRequestId`] to help the service owner in troubleshooting issues related to their usage of the service.
//! Use [`ServerRequestIdProviderLayer::new_with_response_header`] to use [`ServerRequestId`] in your handler and add it to the response headers.
//!
//! The [`ServerRequestId`] is not meant to be propagated to downstream dependencies of the service. You should rely on a distributed tracing implementation for correlation purposes (e.g. OpenTelemetry).
//!
//! ## Examples
//!
//! Your handler can now optionally take as input a [`ServerRequestId`].
//!
//! ```rust,ignore
//! pub async fn handler(
//!     _input: Input,
//!     server_request_id: ServerRequestId,
//! ) -> Output {
//!     /* Use server_request_id */
//!     todo!()
//! }
//!
//! let app = Service::builder_without_plugins()
//!     .operation(handler)
//!     .build().unwrap();
//!
//! let app = app
//!     .layer(&ServerRequestIdProviderLayer::new_with_response_header(HeaderName::from_static("x-request-id"))); /* Generate a server request ID and add it to the response header */
//!
//! let bind: std::net::SocketAddr = format!("{}:{}", args.address, args.port)
//!     .parse()
//!     .expect("unable to parse the server bind address and port");
//! let server = hyper::Server::bind(&bind).serve(app.into_make_service());
//! ```

use std::future::Future;
use std::{
    fmt::Display,
    task::{Context, Poll},
};

use futures_util::TryFuture;
use http::request::Parts;
use http::{header::HeaderName, HeaderValue, Response};
use thiserror::Error;
use tower::{Layer, Service};
use uuid::Uuid;

use crate::{body::BoxBody, response::IntoResponse};

use super::{internal_server_error, FromParts};

/// Opaque type for Server Request IDs.
///
/// If it is missing, the request will be rejected with a `500 Internal Server Error` response.
#[derive(Clone, Debug)]
pub struct ServerRequestId {
    id: Uuid,
}

/// The server request ID has not been added to the [`Request`](http::Request) or has been previously removed.
#[non_exhaustive]
#[derive(Debug, Error)]
#[error("the `ServerRequestId` is not present in the `http::Request`")]
pub struct MissingServerRequestId;

impl ServerRequestId {
    pub fn new() -> Self {
        Self { id: Uuid::new_v4() }
    }

    pub(crate) fn to_header(&self) -> HeaderValue {
        HeaderValue::from_str(&self.id.to_string()).expect("This string contains only valid ASCII")
    }
}

impl Display for ServerRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
    }
}

impl<P> FromParts<P> for ServerRequestId {
    type Rejection = MissingServerRequestId;

    fn from_parts(parts: &mut Parts) -> Result<Self, Self::Rejection> {
        parts.extensions.remove().ok_or(MissingServerRequestId)
    }
}

impl Default for ServerRequestId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct ServerRequestIdProvider<S> {
    inner: S,
    header_key: Option<HeaderName>,
}

/// A layer that provides services with a unique request ID instance
#[derive(Debug)]
#[non_exhaustive]
pub struct ServerRequestIdProviderLayer {
    header_key: Option<HeaderName>,
}

impl ServerRequestIdProviderLayer {
    /// Generate a new unique request ID and do not add it as a response header
    /// Use [`ServerRequestIdProviderLayer::new_with_response_header`] to also add it as a response header
    pub fn new() -> Self {
        Self { header_key: None }
    }

    /// Generate a new unique request ID and add it as a response header
    pub fn new_with_response_header(header_key: HeaderName) -> Self {
        Self {
            header_key: Some(header_key),
        }
    }
}

impl Default for ServerRequestIdProviderLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for ServerRequestIdProviderLayer {
    type Service = ServerRequestIdProvider<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ServerRequestIdProvider {
            inner,
            header_key: self.header_key.clone(),
        }
    }
}

impl<Body, S> Service<http::Request<Body>> for ServerRequestIdProvider<S>
where
    S: Service<http::Request<Body>, Response = Response<crate::body::BoxBody>>,
    S::Future: std::marker::Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ServerRequestIdResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<Body>) -> Self::Future {
        let request_id = ServerRequestId::new();
        match &self.header_key {
            Some(header_key) => {
                req.extensions_mut().insert(request_id.clone());
                ServerRequestIdResponseFuture {
                    response_package: Some(ResponsePackage {
                        request_id,
                        header_key: header_key.clone(),
                    }),
                    fut: self.inner.call(req),
                }
            }
            None => {
                req.extensions_mut().insert(request_id);
                ServerRequestIdResponseFuture {
                    response_package: None,
                    fut: self.inner.call(req),
                }
            }
        }
    }
}

impl<Protocol> IntoResponse<Protocol> for MissingServerRequestId {
    fn into_response(self) -> http::Response<BoxBody> {
        internal_server_error()
    }
}

struct ResponsePackage {
    request_id: ServerRequestId,
    header_key: HeaderName,
}

pin_project_lite::pin_project! {
    pub struct ServerRequestIdResponseFuture<Fut> {
        response_package: Option<ResponsePackage>,
        #[pin]
        fut: Fut,
    }
}

impl<Fut> Future for ServerRequestIdResponseFuture<Fut>
where
    Fut: TryFuture<Ok = Response<crate::body::BoxBody>>,
{
    type Output = Result<Fut::Ok, Fut::Error>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let fut = this.fut;
        let response_package = this.response_package;
        fut.try_poll(cx).map_ok(|mut res| {
            if let Some(response_package) = response_package.take() {
                res.headers_mut()
                    .insert(response_package.header_key, response_package.request_id.to_header());
            }
            res
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::{Body, BoxBody};
    use crate::request::Request;
    use http::HeaderValue;
    use std::convert::Infallible;
    use tower::{service_fn, ServiceBuilder, ServiceExt};

    #[test]
    fn test_request_id_parsed_by_header_value_infallible() {
        ServerRequestId::new().to_header();
    }

    #[tokio::test]
    async fn test_request_id_in_response_header() {
        let svc = ServiceBuilder::new()
            .layer(&ServerRequestIdProviderLayer::new_with_response_header(
                HeaderName::from_static("x-request-id"),
            ))
            .service(service_fn(|_req: Request<Body>| async move {
                Ok::<_, Infallible>(Response::new(BoxBody::default()))
            }));

        let req = Request::new(Body::empty());

        let res = svc.oneshot(req).await.unwrap();
        let request_id = res.headers().get("x-request-id").unwrap().to_str().unwrap();

        assert!(HeaderValue::from_str(request_id).is_ok());
    }

    #[tokio::test]
    async fn test_request_id_not_in_response_header() {
        let svc = ServiceBuilder::new()
            .layer(&ServerRequestIdProviderLayer::new())
            .service(service_fn(|_req: Request<Body>| async move {
                Ok::<_, Infallible>(Response::new(BoxBody::default()))
            }));

        let req = Request::new(Body::empty());

        let res = svc.oneshot(req).await.unwrap();

        assert!(res.headers().is_empty());
    }
}
