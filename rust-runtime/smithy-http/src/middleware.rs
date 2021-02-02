/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! This modules defines the core, framework agnostic, HTTP middleware interface
//! used by the SDK
//!
//! smithy-middleware-tower provides Tower-specific middleware utilities (todo)

use crate::operation;
use crate::pin_mut;
use crate::response::ParseHttpResponse;
use crate::result::{SdkError, SdkSuccess};
use bytes::{Buf, Bytes};
use http_body::Body;
use std::error::Error;
use std::fmt::Debug;

type BoxError = Box<dyn Error + Send + Sync>;

/// [`MapRequest`] defines a synchronous middleware that transforms an [`operation::Request`].
///
/// Typically, these middleware will read configuration from the `PropertyBag` and use it to
/// augment the request. Most fundamental middleware is expressed as `MapRequest`, including
/// signing & endpoint resolution.
///
/// ```rust
/// # use smithy_http::middleware::MapRequest;
/// # use std::convert::Infallible;
/// # use smithy_http::operation;
/// use http::header::{HeaderName, HeaderValue};
/// struct AddHeader(HeaderName, HeaderValue);
/// /// Signaling struct added to the request property bag if a header should be added
/// struct NeedsHeader;
/// impl MapRequest for AddHeader {
///     type Error = Infallible;
///     fn apply(&self, request: operation::Request) -> Result<operation::Request, Self::Error> {
///         request.augment(|mut request, properties| {
///             if properties.get::<NeedsHeader>().is_some() {
///                 request.headers_mut().append(
///                     self.0.clone(),
///                     self.1.clone(),
///                 );
///             }
///             Ok(request)
///         })
///     }
/// }
/// ```
pub trait MapRequest {
    /// The Error type returned by this operation.
    ///
    /// If this middleware never fails use [std::convert::Infallible] or similar.
    type Error: Into<BoxError>;

    /// Apply this middleware to a request.
    ///
    /// Typically, implementations will use [`request.augment`](crate::operation::Request::augment)
    /// to be able to transform an owned `http::Request`.
    fn apply(&self, request: operation::Request) -> Result<operation::Request, Self::Error>;
}

/// Load a response using `handler` to parse the results.
///
/// This function is intended to be used on the response side of a middleware chain.
///
/// Success and failure will be split and mapped into `SdkSuccess` and `SdkError`.
/// Generic Parameters:
/// - `B`: The Response Body
/// - `O`: The Http response handler that returns `Result<T, E>`
/// - `T`/`E`: `Result<T, E>` returned by `handler`.
pub async fn load_response<B, T, E, O>(
    mut response: http::Response<B>,
    handler: &O,
) -> Result<SdkSuccess<T>, SdkError<E>>
where
    B: http_body::Body + Unpin,
    B: From<Bytes> + Debug + 'static,
    B::Error: Error + Send + Sync + 'static,
    O: ParseHttpResponse<B, Output = Result<T, E>>,
{
    if let Some(parsed_response) = handler.parse_unloaded(&mut response) {
        return sdk_result(parsed_response, response);
    }

    let body = match read_body(response.body_mut()).await {
        Ok(body) => body,
        Err(e) => {
            return Err(SdkError::ResponseError {
                raw: response.map(|b| Box::new(b) as _),
                err: Box::new(e),
            });
        }
    };

    let response = response.map(|_| Bytes::from(body));
    let parsed = handler.parse_loaded(&response);
    return sdk_result(parsed, response.map(B::from));
}

async fn read_body<B: http_body::Body>(body: B) -> Result<Vec<u8>, B::Error> {
    let mut output = Vec::new();
    pin_mut!(body);
    while let Some(buf) = body.data().await {
        let mut buf = buf?;
        while buf.has_remaining() {
            output.extend_from_slice(buf.chunk());
            buf.advance(buf.chunk().len())
        }
    }
    Ok(output)
}

fn sdk_result<T, E, B: Debug + 'static>(
    parsed: Result<T, E>,
    raw: http::Response<B>,
) -> Result<SdkSuccess<T>, SdkError<E>> {
    let dyn_body = raw.map(|b| Box::new(b) as _);
    match parsed {
        Ok(parsed) => Ok(SdkSuccess {
            raw: dyn_body,
            parsed,
        }),
        Err(err) => Err(SdkError::ServiceError { raw: dyn_body, err }),
    }
}
