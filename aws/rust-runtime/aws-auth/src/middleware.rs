/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use crate::provider::CredentialsProvider;
use smithy_http::middleware::AsyncMapRequest;
use smithy_http::operation::Request;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Default)]
#[non_exhaustive]
pub struct CredentialsStage;

impl CredentialsStage {
    pub fn new() -> Self {
        CredentialsStage
    }
}

mod error {
    use crate::provider::CredentialsError;
    use std::error::Error as StdError;
    use std::fmt;

    #[derive(Debug)]
    pub enum CredentialsStageError {
        MissingCredentialsProvider,
        CredentialsLoadingError(CredentialsError),
    }

    impl StdError for CredentialsStageError {}

    impl fmt::Display for CredentialsStageError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            use CredentialsStageError::*;
            match self {
                MissingCredentialsProvider => {
                    write!(f, "No credentials provider in the property bag")
                }
                CredentialsLoadingError(err) => write!(
                    f,
                    "Failed to load credentials from the credentials provider: {}",
                    err
                ),
            }
        }
    }

    impl From<CredentialsError> for CredentialsStageError {
        fn from(err: CredentialsError) -> Self {
            CredentialsStageError::CredentialsLoadingError(err)
        }
    }
}

pub use error::*;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl AsyncMapRequest for CredentialsStage {
    type Error = CredentialsStageError;

    fn apply(&self, mut request: Request) -> BoxFuture<Result<Request, Self::Error>> {
        Box::pin(async move {
            let cred_future = {
                let config = request.config();
                let credential_provider = config
                    .get::<CredentialsProvider>()
                    .ok_or(CredentialsStageError::MissingCredentialsProvider)?;
                credential_provider.provide_credentials()
            };
            let credentials = cred_future.await?;
            request.config_mut().insert(credentials);
            Ok(request)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CredentialsStage;
    use crate::provider::set_provider;
    use crate::Credentials;
    use smithy_http::body::SdkBody;
    use smithy_http::middleware::AsyncMapRequest;
    use smithy_http::operation;
    use std::sync::Arc;

    #[tokio::test]
    async fn async_map_request() {
        let stage = CredentialsStage::new();
        let req = operation::Request::new(http::Request::new(SdkBody::from("some body")));
        stage
            .apply(req)
            .await
            .expect_err("should fail if there's no credential provider in the bag");

        let mut req = operation::Request::new(http::Request::new(SdkBody::from("some body")));
        set_provider(
            &mut req.config_mut(),
            Arc::new(Credentials::from_keys("test", "test", None)),
        );
        let req = stage
            .apply(req)
            .await
            .expect("credential provider is in the bag; should succeed");
        assert!(
            req.config().get::<Credentials>().is_some(),
            "it should set credentials on the request config"
        );
    }
}
