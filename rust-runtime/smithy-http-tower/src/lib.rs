pub mod dispatch;
pub mod map_request;
pub mod parse_response;

use tower::BoxError;
use smithy_http::result::SdkError;

/// An Error Occurred During the process of sending an Operation
///
/// The variants are split to enable the final [SdkError](`smithy_http::result::SdkError`) to differentiate
/// between errors that were never sent across the wire (eg. because a region wasn't set) and errors that failed
/// to send (eg. because the hostname couldn't be resolved).
///
/// `SendOperationError` is currently defined only in `smithy-http-tower` because it may be removed
/// or replaced with `SdkError` in the future.
#[derive(Debug)]
pub enum SendOperationError {
    /// The request could not be constructed
    RequestConstructionError(BoxError),

    /// The request could not be dispatched
    RequestDispatchError(BoxError),
}

/// Convert a `SendOperationError` into an `SdkError`
impl<E, B> From<SendOperationError> for SdkError<E, B> {
    fn from(err: SendOperationError) -> Self {
        match err {
            SendOperationError::RequestDispatchError(e) => {
                smithy_http::result::SdkError::DispatchFailure(e.into())
            }
            SendOperationError::RequestConstructionError(e) => {
                smithy_http::result::SdkError::ConstructionFailure(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dispatch::DispatchLayer;
    use crate::map_request::MapRequestLayer;
    use crate::parse_response::ParseResponseLayer;
    use bytes::Bytes;
    use http::Response;
    use smithy_http::body::SdkBody;
    use smithy_http::middleware::MapRequest;
    use smithy_http::operation;
    use smithy_http::operation::{Operation, Request};
    use smithy_http::response::ParseStrictResponse;
    use std::convert::{Infallible, TryInto};
    use tower::{service_fn, Service, ServiceBuilder};

    /// Creates a stubbed service stack and runs it to validate that all the types line up &
    /// everything is properly wired
    #[tokio::test]
    async fn service_stack() {
        #[derive(Clone)]
        struct AddHeader;
        impl MapRequest for AddHeader {
            type Error = Infallible;
            fn apply(&self, request: Request) -> Result<Request, Self::Error> {
                request.augment(|mut req, _| {
                    req.headers_mut()
                        .insert("X-Test", "Value".try_into().unwrap());
                    Ok(req)
                })
            }
        }

        struct TestParseResponse;
        impl ParseStrictResponse for TestParseResponse {
            type Output = Result<String, Infallible>;

            fn parse(&self, _response: &Response<Bytes>) -> Self::Output {
                Ok("OK".to_string())
            }
        }

        let http_layer = service_fn(|_request: http::Request<SdkBody>| async move {
            if _request.headers().contains_key("X-Test") {
                Ok(http::Response::new(SdkBody::from("ok")))
            } else {
                Err("header not set")
            }
        });

        let mut svc = ServiceBuilder::new()
            .layer(ParseResponseLayer::<TestParseResponse>::new())
            .layer(MapRequestLayer::for_mapper(AddHeader))
            .layer(DispatchLayer)
            .service(http_layer);
        let req = http::Request::new(SdkBody::from("hello"));
        let req = operation::Request::new(req);
        let req = Operation::new(req, TestParseResponse);
        let resp = svc.call(req).await.expect("Response should succeed");
        assert_eq!(resp.parsed, "OK".to_string())
    }
}
