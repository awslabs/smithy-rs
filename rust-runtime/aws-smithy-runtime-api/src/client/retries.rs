/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::client::interceptors::context::Error;
use crate::client::interceptors::InterceptorContext;
use crate::client::orchestrator::{BoxError, HttpRequest, HttpResponse};
use crate::config_bag::ConfigBag;
use aws_smithy_types::retry::ErrorKind;
use std::fmt::Debug;
use std::time::Duration;

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
        context: &InterceptorContext<HttpRequest, HttpResponse>,
        cfg: &ConfigBag,
    ) -> Result<ShouldAttempt, BoxError>;
}

#[non_exhaustive]
#[derive(Eq, PartialEq, Debug)]
pub enum RetryReason {
    Error(ErrorKind),
    Explicit(Duration),
}

/// Classifies what kind of retry is needed for a given [`Error`].
pub trait ClassifyRetry: Send + Sync + Debug {
    /// Run this classifier against an error to determine if it should be retried. Returns
    /// `Some(RetryKind)` if the error should be retried; Otherwise returns `None`.
    fn classify_retry(&self, error: &Error) -> Option<RetryReason>;
}

#[derive(Debug)]
pub struct RetryClassifiers {
    inner: Vec<Box<dyn ClassifyRetry>>,
}

impl RetryClassifiers {
    pub fn new() -> Self {
        Self {
            inner: Vec::with_capacity(1),
        }
    }

    pub fn with_classifier(mut self, retry_classifier: impl ClassifyRetry + 'static) -> Self {
        self.inner.push(Box::new(retry_classifier));

        self
    }

    // TODO make a map function so users can front-run or second-guess the classifier's decision
    // pub fn map_classifiers(mut self, fun: Fn() -> RetryClassifiers)
}

impl ClassifyRetry for RetryClassifiers {
    fn classify_retry(&self, error: &Error) -> Option<RetryReason> {
        let mut retry_reason = None;

        for retry_classifier in self.inner.iter() {
            retry_reason = retry_classifier.classify_retry(error);

            if retry_reason.is_some() {
                break;
            }
        }

        retry_reason
    }
}
