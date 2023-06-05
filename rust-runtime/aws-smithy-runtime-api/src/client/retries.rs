/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::client::interceptors::InterceptorContext;
use crate::client::orchestrator::BoxError;
use aws_smithy_types::config_bag::ConfigBag;
use std::fmt::Debug;
use std::time::Duration;
use tracing::trace;

pub use aws_smithy_types::retry::ErrorKind;

#[derive(Debug, Clone, PartialEq, Eq)]
/// An answer to the question "should I make a request attempt?"
pub enum ShouldAttempt {
    Yes,
    No,
    YesAfterDelay(Duration),
}

pub trait RetryStrategy: Send + Sync + Debug {
    fn should_attempt_initial_request(&self, cfg: &ConfigBag) -> Result<ShouldAttempt, BoxError>;

    fn should_attempt_retry(
        &self,
        context: &InterceptorContext,
        cfg: &ConfigBag,
    ) -> Result<ShouldAttempt, BoxError>;
}

#[non_exhaustive]
#[derive(Eq, PartialEq, Debug)]
pub enum RetryReason {
    Error(ErrorKind),
    Explicit(Duration),
}

/// Classifies what kind of retry is needed for a given an [`InterceptorContext`].
pub trait ClassifyRetry: Send + Sync + Debug {
    /// Run this classifier against an error to determine if it should be retried. Returns
    /// `Some(RetryKind)` if the error should be retried; Otherwise returns `None`.
    fn classify_retry(&self, ctx: &InterceptorContext) -> Option<RetryReason>;

    /// The name that this classifier should report for debugging purposes.
    fn name(&self) -> &'static str;
}

#[derive(Debug)]
pub struct RetryClassifiers {
    inner: Vec<Box<dyn ClassifyRetry>>,
}

impl RetryClassifiers {
    pub fn new() -> Self {
        Self {
            // It's always expected that at least one classifier will be defined,
            // so we eagerly allocate for it.
            inner: Vec::with_capacity(1),
        }
    }

    pub fn with_classifier(mut self, retry_classifier: impl ClassifyRetry + 'static) -> Self {
        self.inner.push(Box::new(retry_classifier));

        self
    }

    // TODO(https://github.com/awslabs/smithy-rs/issues/2632) make a map function so users can front-run or second-guess the classifier's decision
    // pub fn map_classifiers(mut self, fun: Fn() -> RetryClassifiers)
}

impl ClassifyRetry for RetryClassifiers {
    fn classify_retry(&self, error: &InterceptorContext) -> Option<RetryReason> {
        // return the first non-None result
        self.inner.iter().find_map(|cr| {
            let maybe_reason = cr.classify_retry(error);

            match maybe_reason.as_ref() {
                Some(reason) => trace!(
                    "\"{}\" classifier classified error as {:?}",
                    cr.name(),
                    reason
                ),
                None => trace!("\"{}\" classifier ignored the error", cr.name()),
            };

            maybe_reason
        })
    }

    fn name(&self) -> &'static str {
        "Collection of Classifiers"
    }
}

#[cfg(feature = "test-util")]
mod test_util {
    use super::{ClassifyRetry, ErrorKind, RetryReason};
    use crate::client::interceptors::InterceptorContext;
    use tracing::trace;

    /// A retry classifier for testing purposes. This classifier always returns
    /// `Some(RetryReason::Error(ErrorKind))` where `ErrorKind` is the value provided when creating
    /// this classifier.
    #[derive(Debug)]
    pub struct AlwaysRetry(pub ErrorKind);

    impl ClassifyRetry for AlwaysRetry {
        fn classify_retry(&self, error: &InterceptorContext) -> Option<RetryReason> {
            trace!("Retrying error {:?} as an {:?}", error, self.0);
            Some(RetryReason::Error(self.0))
        }

        fn name(&self) -> &'static str {
            "Always Retry"
        }
    }
}

#[cfg(feature = "test-util")]
pub use test_util::AlwaysRetry;
