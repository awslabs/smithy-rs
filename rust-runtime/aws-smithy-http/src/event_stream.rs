/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Provides Sender/Receiver implementations for Event Stream codegen.

use std::{error::Error as StdError, fmt::Debug, marker::PhantomData};

mod receiver;
mod sender;

pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;

use aws_smithy_eventstream::frame::UnmarshallMessage;
use bytes::Bytes;
#[doc(inline)]
pub use sender::{EventStreamSender, MessageStreamAdapter, MessageStreamError};

#[doc(inline)]
pub use receiver::{Error, RawMessage, Receiver};

use crate::result::SdkError;

/// This data is used to fill a field when the users try to deserialize data that has a Receiver in one of the field.
pub struct DeserializedReceiverStream<T, E>(PhantomData<(T, E)>);
impl<T, E> Debug for DeserializedReceiverStream<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeserializedStream")
    }
}
impl<T, E> DeserializedReceiverStream<T, E>
where
    T: std::fmt::Debug,
    E: std::fmt::Debug,
{
    /// default value
    pub fn new() -> impl UnmarshallMessage<Output = T, Error = E> {
        Self(PhantomData::<(T, E)>)
    }
}

impl<T, E> UnmarshallMessage for DeserializedReceiverStream<T, E>
where
    T: Debug,
    E: Debug,
{
    type Error = E;
    type Output = T;
    fn unmarshall(
        &self,
        _: &aws_smithy_eventstream::frame::Message,
    ) -> Result<
        aws_smithy_eventstream::frame::UnmarshalledMessage<Self::Output, Self::Error>,
        aws_smithy_eventstream::error::Error,
    > {
        Err(aws_smithy_eventstream::error::Error::Unmarshalling(
            "DeserializedStream".to_string(),
        ))
    }
}

/// Error returned from Deserialized Stream.
#[derive(Debug)]
struct DeserializedStreamError;

impl StdError for DeserializedStreamError {}
impl std::fmt::Display for DeserializedStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Stream was deserialized")
    }
}
