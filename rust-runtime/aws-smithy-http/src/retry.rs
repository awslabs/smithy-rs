/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! HTTP specific retry behaviors
//!
//! For protocol agnostic retries, see `aws_smithy_types::Retry`.

use crate::operation::Response;
use crate::result::SdkError;
use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind, RetryKind};

/// Classifies what kind of retry is needed for a given `response`.
pub trait ClassifyResponse<T, E>: Clone {
    fn classify(&self, response: Result<&T, &E>) -> RetryKind;
}

const TRANSIENT_ERROR_STATUS_CODES: &[u16] = &[500, 502, 503, 504];

/// The default implementation of [`ClassifyResponse`] for generated clients.
#[derive(Clone, Debug, Default)]
pub struct DefaultResponseClassifier;

impl DefaultResponseClassifier {
    /// Creates a new `DefaultResponseClassifier`
    pub fn new() -> Self {
        Default::default()
    }

    /// Matches on the given `result` and, if possible, returns the underlying cause and the operation response
    /// that can be used for further classification logic. Otherwise, it returns a `RetryKind` that should be used
    /// for the result.
    #[doc(hidden)]
    pub fn try_extract_err_response<'a, T, E>(
        result: Result<&T, &'a SdkError<E>>,
    ) -> Result<(&'a E, &'a Response), RetryKind> {
        match result {
            Ok(_) => Err(RetryKind::Unnecessary),
            Err(SdkError::ServiceError { err, raw }) => Ok((err, raw)),
            Err(SdkError::TimeoutError(_err)) => Err(RetryKind::Error(ErrorKind::TransientError)),
            Err(SdkError::DispatchFailure(err)) => {
                if err.is_timeout() || err.is_io() {
                    Err(RetryKind::Error(ErrorKind::TransientError))
                } else if let Some(ek) = err.is_other() {
                    Err(RetryKind::Error(ek))
                } else {
                    Err(RetryKind::UnretryableFailure)
                }
            }
            Err(_) => Err(RetryKind::UnretryableFailure),
        }
    }
}

impl<T, E> ClassifyResponse<T, SdkError<E>> for DefaultResponseClassifier
where
    E: ProvideErrorKind,
{
    fn classify(&self, result: Result<&T, &SdkError<E>>) -> RetryKind {
        let (err, response) = match Self::try_extract_err_response(result) {
            Ok(extracted) => extracted,
            Err(retry_kind) => return retry_kind,
        };
        if let Some(kind) = err.retryable_error_kind() {
            return RetryKind::Error(kind);
        };
        if TRANSIENT_ERROR_STATUS_CODES.contains(&response.http().status().as_u16()) {
            return RetryKind::Error(ErrorKind::TransientError);
        };
        RetryKind::UnretryableFailure
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::body::SdkBody;
    use crate::operation;
    use crate::result::{SdkError, SdkSuccess};
    use crate::retry::ClassifyResponse;
    use aws_smithy_types::retry::{ErrorKind, ProvideErrorKind, RetryKind};

    struct UnmodeledError;

    struct CodedError {
        code: &'static str,
    }

    impl ProvideErrorKind for UnmodeledError {
        fn retryable_error_kind(&self) -> Option<ErrorKind> {
            None
        }

        fn code(&self) -> Option<&str> {
            None
        }
    }

    impl ProvideErrorKind for CodedError {
        fn retryable_error_kind(&self) -> Option<ErrorKind> {
            None
        }

        fn code(&self) -> Option<&str> {
            Some(self.code)
        }
    }

    fn make_err<E>(
        err: E,
        raw: http::Response<&'static str>,
    ) -> Result<SdkSuccess<()>, SdkError<E>> {
        Err(SdkError::ServiceError {
            err,
            raw: operation::Response::new(raw.map(|b| SdkBody::from(b))),
        })
    }

    #[test]
    fn not_an_error() {
        let policy = DefaultResponseClassifier::new();
        let test_response = http::Response::new("OK");
        assert_eq!(
            policy.classify(make_err(UnmodeledError, test_response).as_ref()),
            RetryKind::UnretryableFailure
        );
    }

    #[test]
    fn classify_by_response_status() {
        let policy = DefaultResponseClassifier::new();
        let test_resp = http::Response::builder()
            .status(500)
            .body("error!")
            .unwrap();
        assert_eq!(
            policy.classify(make_err(UnmodeledError, test_resp).as_ref()),
            RetryKind::Error(ErrorKind::TransientError)
        );
    }

    #[test]
    fn classify_by_response_status_not_retryable() {
        let policy = DefaultResponseClassifier::new();
        let test_resp = http::Response::builder()
            .status(408)
            .body("error!")
            .unwrap();
        assert_eq!(
            policy.classify(make_err(UnmodeledError, test_resp).as_ref()),
            RetryKind::UnretryableFailure
        );
    }

    #[test]
    fn classify_by_error_kind() {
        struct ModeledRetries;
        let test_response = http::Response::new("OK");
        impl ProvideErrorKind for ModeledRetries {
            fn retryable_error_kind(&self) -> Option<ErrorKind> {
                Some(ErrorKind::ClientError)
            }

            fn code(&self) -> Option<&str> {
                // code should not be called when `error_kind` is provided
                unimplemented!()
            }
        }

        let policy = DefaultResponseClassifier::new();

        assert_eq!(
            policy.classify(make_err(ModeledRetries, test_response).as_ref()),
            RetryKind::Error(ErrorKind::ClientError)
        );
    }

    #[test]
    fn test_timeout_error() {
        let policy = DefaultResponseClassifier::new();
        let err: Result<(), SdkError<UnmodeledError>> = Err(SdkError::TimeoutError("blah".into()));
        assert_eq!(
            policy.classify(err.as_ref()),
            RetryKind::Error(ErrorKind::TransientError)
        );
    }
}
