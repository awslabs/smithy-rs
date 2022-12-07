/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! # Request IDs
//!
//! RequestID is an element that uniquely identifies a client request. RequestID is used by services to map all logs, events and
//! specific data to a single operation. This RFC discusses whether and how smithy-rs can make that value available to customers.
//!
//! Services use a RequestID to collect logs related to the same request and see its flow through the various operations,
//! help clients debug requests by sharing this value and, in some cases, use this value to perform their business logic. RequestID is unique across a service at least within a certain timeframe.
//!
//! RequestIDs are not to be used by multiple services, but only within a single service.
//!
//! For client request IDs, the process will be, in order:
//! * If a header is found matching one of the possible ones, use it
//! * Otherwise, None
//!
//! Server request IDs are opaque and generated by the service itself.
//!
//! Your handler can now optionally take as input a [`ServerRequestId`] and an [`Option<ClientRequestId>`].
//!
//! ```rust,ignore
//! pub async fn handler(
//!     _input: Input,
//!     server_request_id: ServerRequestId,
//!     client_request_id: Option<ClientRequestId>,
//! ) -> Output {
//!     /* Use server_request_id and client_request_id */
//!     todo!()
//! }
//!
//! let app = Service::builder_without_plugins()
//!     .operation(handler)
//!     .build().unwrap();
//!
//! let app = app.layer(&ServerRequestIdProviderLayer::new()); /* Generate a server request ID */
//! let app = app.layer(&ClientRequestIdProviderLayer::new(&["x-request-id"])); /* Provide your handler with the client request ID */
//!
//! let bind: std::net::SocketAddr = format!("{}:{}", args.address, args.port)
//!     .parse()
//!     .expect("unable to parse the server bind address and port");
//! let server = hyper::Server::bind(&bind).serve(app.into_make_service());
//! ```

use std::{
    borrow::{Borrow, Cow},
    fmt::Display,
    task::{Context, Poll},
};

use http::request::Parts;
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
}

/// A layer that provides services with a unique request ID instance
#[derive(Debug)]
#[non_exhaustive]
pub struct ServerRequestIdProviderLayer;

impl ServerRequestIdProviderLayer {
    /// Generate a new unique request ID
    pub fn new() -> Self {
        Self {}
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
        ServerRequestIdProvider { inner }
    }
}

impl<Body, S> Service<http::Request<Body>> for ServerRequestIdProvider<S>
where
    S: Service<http::Request<Body>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<Body>) -> Self::Future {
        req.extensions_mut().insert(ServerRequestId::new());
        self.inner.call(req)
    }
}

impl<Protocol> IntoResponse<Protocol> for MissingServerRequestId {
    fn into_response(self) -> http::Response<BoxBody> {
        internal_server_error()
    }
}

/// The Client Request ID.
///
/// If it is missing, the request will be rejected with a `500 Internal Server Error` response.
#[derive(Clone, Debug)]
pub struct ClientRequestId {
    id: Box<str>,
}

/// The client request ID has not been added to the [`Request`](http::Request) or has been previously removed.
#[non_exhaustive]
#[derive(Debug, Error)]
#[error("the `ClientRequestId` is not present in the `http::Request`")]
pub struct MissingClientRequestId;

impl ClientRequestId {
    /// Wrap an incoming request ID from a client
    pub fn new(id: Box<str>) -> Self {
        Self { id }
    }
}

impl Display for ClientRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
    }
}

impl<P> FromParts<P> for Option<ClientRequestId> {
    type Rejection = MissingClientRequestId;

    fn from_parts(parts: &mut Parts) -> Result<Self, Self::Rejection> {
        parts.extensions.remove::<Self>().ok_or(MissingClientRequestId)
    }
}

#[derive(Clone)]
pub struct ClientRequestIdProvider<'a, S> {
    inner: S,
    possible_headers: &'a [Cow<'a, str>],
}

pub struct ClientRequestIdProviderLayer<'a> {
    possible_headers: &'a [Cow<'a, str>],
}

impl<'a> ClientRequestIdProviderLayer<'a> {
    pub fn new(possible_headers: &'a [Cow<'a, str>]) -> Self {
        Self { possible_headers }
    }
}

impl<'a, S> Layer<S> for ClientRequestIdProviderLayer<'a> {
    type Service = ClientRequestIdProvider<'a, S>;

    fn layer(&self, inner: S) -> Self::Service {
        ClientRequestIdProvider {
            inner,
            possible_headers: self.possible_headers,
        }
    }
}

impl<'a, R, S> Service<http::Request<R>> for ClientRequestIdProvider<'a, S>
where
    S: Service<http::Request<R>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<R>) -> Self::Future {
        let mut id: Option<ClientRequestId> = None;
        for possible_header in self.possible_headers {
            let possible_header: &'a str = possible_header.borrow();
            if let Some(value) = req.headers().get(possible_header) {
                if let Ok(value) = value.to_str() {
                    id = Some(ClientRequestId::new(value.into()));
                    break;
                }
            }
        }
        req.extensions_mut().insert(id);
        self.inner.call(req)
    }
}

impl<Protocol> IntoResponse<Protocol> for MissingClientRequestId {
    fn into_response(self) -> http::Response<BoxBody> {
        internal_server_error()
    }
}
