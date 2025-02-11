/*
 *  Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 *  SPDX-License-Identifier: Apache-2.0
 */

use aws_smithy_http::event_stream::Receiver;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::event_stream::{Message, RawMessage};

#[derive(Debug)]
/// Receives unmarshalled events at a time out of an Event Stream.
pub struct EventReceiver<T, E> {
    inner: Receiver<T, E>,
}

impl<T, E> EventReceiver<T, E> {
    pub(crate) fn new(inner: Receiver<T, E>) -> Self {
        Self { inner }
    }

    // Wrapper around `try_recv_initial` on `aws_smithy_http::event_stream::Receiver`
    //
    // Note: This method is intended for internal use only.
    pub(crate) async fn try_recv_initial(
        &mut self,
    ) -> Result<Option<Message>, SdkError<E, RawMessage>> {
        self.inner.try_recv_initial().await
    }

    /// Asynchronously tries to receive an event from the stream. If the stream has ended, it
    /// returns an `Ok(None)`. If there is a transport layer error, it will return
    /// `Err(SdkError::DispatchFailure)`. Service-modeled errors will be a part of the returned
    /// messages.
    pub async fn recv(&mut self) -> Result<Option<T>, SdkError<E, RawMessage>> {
        self.inner.recv().await
    }
}
