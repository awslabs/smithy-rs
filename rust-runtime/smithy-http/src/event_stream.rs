/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Provides Sender/Receiver implementations for Event Stream codegen.

use crate::body::SdkBody;
use crate::result::SdkError;
use bytes::Bytes;
use bytes_utils::SegmentedBuf;
use futures_core::Stream;
use hyper::body::HttpBody;
use pin_project::pin_project;
use smithy_eventstream::frame::{
    DecodedFrame, MarshallMessage, MessageFrameDecoder, SignMessage, UnmarshallMessage,
    UnmarshalledMessage,
};
use std::error::Error as StdError;
use std::fmt;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;

/// Input type for Event Streams.
pub struct EventStreamInput<T> {
    input_stream: Pin<Box<dyn Stream<Item = Result<T, BoxError>> + Send>>,
}

impl<T> fmt::Debug for EventStreamInput<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EventStreamInput(Box<dyn Stream>)")
    }
}

impl<T> EventStreamInput<T> {
    #[doc(hidden)]
    pub fn into_body_stream<E: StdError + Send + Sync + 'static>(
        self,
        marshaller: impl MarshallMessage<Input = T> + Send + Sync + 'static,
        signer: impl SignMessage + Send + Sync + 'static,
    ) -> MessageStreamAdapter<T, E> {
        MessageStreamAdapter::new(marshaller, signer, self.input_stream)
    }
}

impl<T, S> From<S> for EventStreamInput<T>
where
    S: Stream<Item = Result<T, BoxError>> + Send + 'static,
{
    fn from(stream: S) -> Self {
        EventStreamInput {
            input_stream: Box::pin(stream),
        }
    }
}

/// Adapts a `Stream<SmithyMessageType>` to a signed `Stream<Bytes>` by using the provided
/// message marshaller and signer implementations.
///
/// This will yield an `Err(SdkError::ConstructionFailure)` if a message can't be
/// marshalled into an Event Stream frame, (e.g., if the message payload was too large).
#[pin_project]
pub struct MessageStreamAdapter<T, E> {
    marshaller: Box<dyn MarshallMessage<Input = T> + Send + Sync>,
    signer: Box<dyn SignMessage + Send + Sync>,
    #[pin]
    stream: Pin<Box<dyn Stream<Item = Result<T, BoxError>> + Send>>,
    _phantom: PhantomData<E>,
}

impl<T, E> MessageStreamAdapter<T, E>
where
    E: StdError + Send + Sync + 'static,
{
    pub fn new(
        marshaller: impl MarshallMessage<Input = T> + Send + Sync + 'static,
        signer: impl SignMessage + Send + Sync + 'static,
        stream: Pin<Box<dyn Stream<Item = Result<T, BoxError>> + Send>>,
    ) -> Self {
        MessageStreamAdapter {
            marshaller: Box::new(marshaller),
            signer: Box::new(signer),
            stream,
            _phantom: Default::default(),
        }
    }
}

impl<T, E> Stream for MessageStreamAdapter<T, E>
where
    E: StdError + Send + Sync + 'static,
{
    type Item = Result<Bytes, SdkError<E>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        match this.stream.poll_next(cx) {
            Poll::Ready(message_option) => {
                if let Some(message_result) = message_option {
                    let message_result =
                        message_result.map_err(|err| SdkError::ConstructionFailure(err));
                    let message = this
                        .marshaller
                        .marshall(message_result?)
                        .map_err(|err| SdkError::ConstructionFailure(Box::new(err)))?;
                    let message = this
                        .signer
                        .sign(message)
                        .map_err(|err| SdkError::ConstructionFailure(err))?;
                    let mut buffer = Vec::new();
                    message
                        .write_to(&mut buffer)
                        .map_err(|err| SdkError::ConstructionFailure(Box::new(err)))?;
                    Poll::Ready(Some(Ok(Bytes::from(buffer))))
                } else {
                    Poll::Ready(None)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Receives Smithy-modeled messages out of an Event Stream.
#[derive(Debug)]
pub struct Receiver<T, E> {
    unmarshaller: Box<dyn UnmarshallMessage<Output = T, Error = E>>,
    decoder: MessageFrameDecoder,
    buffer: SegmentedBuf<Bytes>,
    body: SdkBody,
    _phantom: PhantomData<E>,
}

impl<T, E> Receiver<T, E> {
    /// Creates a new `Receiver` with the given message unmarshaller and SDK body.
    pub fn new(
        unmarshaller: impl UnmarshallMessage<Output = T, Error = E> + 'static,
        body: SdkBody,
    ) -> Self {
        Receiver {
            unmarshaller: Box::new(unmarshaller),
            decoder: MessageFrameDecoder::new(),
            buffer: SegmentedBuf::new(),
            body,
            _phantom: Default::default(),
        }
    }

    /// Asynchronously tries to receive a message from the stream. If the stream has ended,
    /// it returns an `Ok(None)`. If there is a transport layer error, it will return
    /// `Err(SdkError::DispatchFailure)`. Service-modeled errors will be a part of the returned
    /// messages.
    pub async fn recv(&mut self) -> Result<Option<T>, SdkError<E>> {
        let next_chunk = self
            .body
            .data()
            .await
            .transpose()
            .map_err(|err| SdkError::DispatchFailure(err))?;
        if let Some(chunk) = next_chunk {
            // The SegmentedBuf will automatically purge when it reads off the end of a chunk boundary
            self.buffer.push(chunk);
            if let DecodedFrame::Complete(message) = self
                .decoder
                .decode_frame(&mut self.buffer)
                .map_err(|err| SdkError::DispatchFailure(Box::new(err)))?
            {
                return match self
                    .unmarshaller
                    .unmarshall(message)
                    .map_err(|err| SdkError::DispatchFailure(Box::new(err)))?
                {
                    UnmarshalledMessage::Event(event) => Ok(Some(event)),
                    UnmarshalledMessage::Error(err) => {
                        Err(SdkError::ServiceError { err, raw: None })
                    }
                };
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::{MarshallMessage, Receiver, UnmarshallMessage};
    use crate::body::SdkBody;
    use crate::event_stream::{EventStreamInput, MessageStreamAdapter};
    use crate::result::SdkError;
    use async_stream::stream;
    use bytes::Bytes;
    use futures_core::Stream;
    use futures_util::stream::StreamExt;
    use hyper::body::Body;
    use smithy_eventstream::error::Error as EventStreamError;
    use smithy_eventstream::frame::{
        Header, HeaderValue, Message, SignMessage, SignMessageError, UnmarshalledMessage,
    };
    use std::error::Error as StdError;
    use std::io::{Error as IOError, ErrorKind};

    fn encode_message(message: &str) -> Bytes {
        let mut buffer = Vec::new();
        Message::new(Bytes::copy_from_slice(message.as_bytes()))
            .write_to(&mut buffer)
            .unwrap();
        buffer.into()
    }

    #[derive(Debug)]
    struct FakeError;
    impl std::fmt::Display for FakeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "FakeError")
        }
    }
    impl StdError for FakeError {}

    #[derive(Debug, Eq, PartialEq)]
    struct TestMessage(String);

    #[derive(Debug)]
    struct Marshaller;
    impl MarshallMessage for Marshaller {
        type Input = TestMessage;

        fn marshall(&self, input: Self::Input) -> Result<Message, EventStreamError> {
            Ok(Message::new(input.0.as_bytes().to_vec()))
        }
    }

    #[derive(Debug)]
    struct Unmarshaller;
    impl UnmarshallMessage for Unmarshaller {
        type Output = TestMessage;
        type Error = EventStreamError;

        fn unmarshall(
            &self,
            message: Message,
        ) -> Result<UnmarshalledMessage<Self::Output, Self::Error>, EventStreamError> {
            Ok(UnmarshalledMessage::Event(TestMessage(
                std::str::from_utf8(&message.payload()[..]).unwrap().into(),
            )))
        }
    }

    #[tokio::test]
    async fn receive_success() {
        let chunks: Vec<Result<_, IOError>> =
            vec![Ok(encode_message("one")), Ok(encode_message("two"))];
        let chunk_stream = futures_util::stream::iter(chunks);
        let body = SdkBody::from(Body::wrap_stream(chunk_stream));
        let mut receiver = Receiver::<TestMessage, EventStreamError>::new(Unmarshaller, body);
        assert_eq!(
            TestMessage("one".into()),
            receiver.recv().await.unwrap().unwrap()
        );
        assert_eq!(
            TestMessage("two".into()),
            receiver.recv().await.unwrap().unwrap()
        );
    }

    #[tokio::test]
    async fn receive_network_failure() {
        let chunks: Vec<Result<_, IOError>> = vec![
            Ok(encode_message("one")),
            Err(IOError::new(ErrorKind::ConnectionReset, FakeError)),
        ];
        let chunk_stream = futures_util::stream::iter(chunks);
        let body = SdkBody::from(Body::wrap_stream(chunk_stream));
        let mut receiver = Receiver::<TestMessage, EventStreamError>::new(Unmarshaller, body);
        assert_eq!(
            TestMessage("one".into()),
            receiver.recv().await.unwrap().unwrap()
        );
        assert!(matches!(
            receiver.recv().await,
            Err(SdkError::DispatchFailure(_))
        ));
    }

    #[tokio::test]
    async fn receive_message_parse_failure() {
        let chunks: Vec<Result<_, IOError>> = vec![
            Ok(encode_message("one")),
            // A zero length message will be invalid. We need to provide a minimum of 12 bytes
            // for the MessageFrameDecoder to actually start parsing it.
            Ok(Bytes::from_static(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])),
        ];
        let chunk_stream = futures_util::stream::iter(chunks);
        let body = SdkBody::from(Body::wrap_stream(chunk_stream));
        let mut receiver = Receiver::<TestMessage, EventStreamError>::new(Unmarshaller, body);
        assert_eq!(
            TestMessage("one".into()),
            receiver.recv().await.unwrap().unwrap()
        );
        assert!(matches!(
            receiver.recv().await,
            Err(SdkError::DispatchFailure(_))
        ));
    }

    #[derive(Debug)]
    struct TestServiceError;
    impl std::fmt::Display for TestServiceError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestServiceError")
        }
    }
    impl StdError for TestServiceError {}

    #[derive(Debug)]
    struct TestSigner;
    impl SignMessage for TestSigner {
        fn sign(&mut self, message: Message) -> Result<Message, SignMessageError> {
            let mut buffer = Vec::new();
            message.write_to(&mut buffer).unwrap();
            Ok(Message::new(buffer).add_header(Header::new("signed", HeaderValue::Bool(true))))
        }
    }

    fn check_compatible_with_hyper_wrap_stream<S, O, E>(stream: S) -> S
    where
        S: Stream<Item = Result<O, E>> + Send + 'static,
        O: Into<Bytes> + 'static,
        E: Into<Box<dyn StdError + Send + Sync>> + 'static,
    {
        stream
    }

    #[tokio::test]
    async fn message_stream_adapter_success() {
        let stream = stream! {
            yield Ok(TestMessage("test".into()));
        };
        let mut adapter =
            check_compatible_with_hyper_wrap_stream(MessageStreamAdapter::<
                TestMessage,
                TestServiceError,
            >::new(
                Marshaller, TestSigner, Box::pin(stream)
            ));

        let mut sent_bytes = adapter.next().await.unwrap().unwrap();
        let sent = Message::read_from(&mut sent_bytes).unwrap();
        assert_eq!("signed", sent.headers()[0].name().as_str());
        assert_eq!(&HeaderValue::Bool(true), sent.headers()[0].value());
        let inner = Message::read_from(&mut (&sent.payload()[..])).unwrap();
        assert_eq!(&b"test"[..], &inner.payload()[..]);
    }

    #[tokio::test]
    async fn message_stream_adapter_construction_failure() {
        let stream = stream! {
            yield Err(EventStreamError::InvalidMessageLength.into());
        };
        let mut adapter =
            check_compatible_with_hyper_wrap_stream(MessageStreamAdapter::<
                TestMessage,
                TestServiceError,
            >::new(
                Marshaller, TestSigner, Box::pin(stream)
            ));

        let result = adapter.next().await.unwrap();
        assert!(result.is_err());
        assert!(matches!(
            result.err().unwrap(),
            SdkError::ConstructionFailure(_)
        ));
    }

    // Verify the developer experience for this compiles
    #[allow(unused)]
    fn event_stream_input_ergonomics() {
        fn check(input: impl Into<EventStreamInput<TestMessage>>) {
            let _: EventStreamInput<TestMessage> = input.into();
        }
        check(stream! {
            yield Ok(TestMessage("test".into()));
        });
        check(stream! {
            yield Err(EventStreamError::InvalidMessageLength.into());
        });
    }
}
