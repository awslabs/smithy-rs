/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Wrap operations in a special type allowing for the modification of operations and the
//! requests inside before sending them.

use crate::client::Handle;

use aws_smithy_http::body::SdkBody;
use aws_smithy_http::operation::Operation;
use aws_smithy_http::response::ParseHttpResponse;
use aws_smithy_http::result::{SdkError, SdkSuccess};
use aws_smithy_http::retry::ClassifyResponse;

use std::convert::Infallible;
use std::sync::Arc;

/// A wrapper type for [`Operation`](aws_smithy_http::operation::Operation)s that allows for
/// customization of the operation before it is sent. A `CustomizableOperation` may be sent
/// by calling its [`.send()`][crate::customizable_operation::CustomizableOperation::send] method.
#[derive(Debug)]
pub struct CustomizableOperation<O, R> {
    pub(crate) handle: Arc<Handle>,
    pub(crate) operation: Operation<O, R>,
}

impl<O, R> CustomizableOperation<O, R> {
    /// Allows for customizing the operation's request
    pub fn map_request<E>(
        mut self,
        f: impl FnOnce(http::Request<SdkBody>) -> Result<http::Request<SdkBody>, E>,
    ) -> Result<Self, E> {
        let (request, response) = self.operation.into_request_response();
        let request = request.augment(|req, _props| f(req))?;
        self.operation = Operation::from_parts(request, response);
        Ok(self)
    }

    /// Convenience for `map_request` where infallible direct mutation of request is acceptable
    pub fn mutate_request<E>(self, f: impl FnOnce(&mut http::Request<SdkBody>)) -> Self {
        self.map_request(|mut req| {
            f(&mut req);
            Result::<_, Infallible>::Ok(req)
        })
        .expect("infallible")
    }

    /// Allows for customizing the entire operation
    pub fn map_operation<E>(
        mut self,
        f: impl FnOnce(Operation<O, R>) -> Result<Operation<O, R>, E>,
    ) -> Result<Self, E> {
        self.operation = f(self.operation)?;
        Ok(self)
    }

    /// Direct access to read the HTTP request
    pub fn request(&self) -> &http::Request<SdkBody> {
        self.operation.request()
    }

    /// Direct access to mutate the HTTP request
    pub fn request_mut(&mut self) -> &mut http::Request<SdkBody> {
        self.operation.request_mut()
    }

    /// Sends this operation's request
    pub async fn send<T, E>(self) -> Result<T, SdkError<E>>
    where
        O: ParseHttpResponse<Output = Result<T, E>> + Send + Sync + Clone + 'static,
        E: std::error::Error,
        R: ClassifyResponse<SdkSuccess<T>, SdkError<E>> + Send + Sync,
    {
        self.handle.client.call(self.operation).await
    }
}
