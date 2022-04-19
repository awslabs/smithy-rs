/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

// This code was copied and then modified from https://github.com/hanabu/lambda-web

use lambda_http::{Error as LambdaError, Request, Response};
use std::{
    convert::Infallible,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tower::Service;

type HyperRequest = hyper::Request<hyper::Body>;
type HyperResponse<B> = hyper::Response<B>;

#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct IntoMakeLambdaService<'a, S> {
    service: S,
    _phantom: PhantomData<&'a ()>,
}

impl<'a, S> IntoMakeLambdaService<'a, S>{
    pub(super) fn new(service: S) -> Self {
        Self {
            service,
            _phantom: PhantomData,
        }
    }
}

impl<'a, S, B> Service<Request> for IntoMakeLambdaService<'a, S>
where
    S: hyper::service::Service<HyperRequest, Response = HyperResponse<B>, Error = Infallible>
        + 'static,
    B: hyper::body::HttpBody,
    <B as hyper::body::HttpBody>::Error: std::error::Error + Send + Sync + 'static,
{
    type Error = LambdaError;
    type Response = HyperResponse<B>;
    type Future = Pin<Box<dyn Future<Output = Result<Response<B>, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Lambda handler function
    /// Parse Lambda event as hyper request,
    /// serialize hyper response to Lambda JSON response
    fn call(&mut self, event: Request) -> Self::Future {
        // Parse request
        let hyper_request = hyper_from_lambda_request(event);

        // Call hyper service when request parsing succeeded
        let svc_call = hyper_request.map(|req| self.service.call(req));

        let fut = async move {
            match svc_call {
                Ok(svc_fut) => {
                    let response = svc_fut.await?;
                    lambda_from_hyper_response(response).await
                }
                Err(request_err) => {
                    // Request parsing error
                    Err(request_err)
                }
            }
        };
        Box::pin(fut)
    }
}

fn hyper_from_lambda_request(event: Request) -> Result<HyperRequest, LambdaError> {
    let (parts, body) = event.into_parts();
    let body = match body {
        lambda_http::Body::Empty => hyper::Body::empty(),
        lambda_http::Body::Text(s) => hyper::Body::from(s),
        lambda_http::Body::Binary(v) => hyper::Body::from(v),
    };
    let req = hyper::Request::from_parts(parts, body);
    Ok(req)
}

async fn lambda_from_hyper_response<B>(response: HyperResponse<B>) -> Result<Response<B>, LambdaError>
where
    B: hyper::body::HttpBody,
    <B as hyper::body::HttpBody>::Error: std::error::Error + Send + Sync + 'static,
{
    // Divide resonse into headers and body
    let (parts, body) = response.into_parts();
    let response = Response::from_parts(parts, body);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traits() {
        use crate::test_helpers::*;

        assert_send::<IntoMakeLambdaService<()>>();
        assert_sync::<IntoMakeLambdaService<()>>();
    }
}
