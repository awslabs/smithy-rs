/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::client::orchestrator::interceptors::RequestAttempts;
use aws_smithy_runtime_api::client::interceptors::InterceptorContext;
use aws_smithy_runtime_api::client::orchestrator::BoxError;
use aws_smithy_runtime_api::client::retries::{
    ClassifyRetry, RetryClassifiers, RetryReason, RetryStrategy, ShouldAttempt,
};
use aws_smithy_runtime_api::config_bag::ConfigBag;
use std::time::Duration;

// A retry policy used in tests. This relies on an error classifier already present in the config bag.
// If a server response is retryable, it will be retried after a fixed delay.
#[derive(Debug, Clone)]
pub struct FixedDelayRetryStrategy {
    fixed_delay: Duration,
    max_attempts: u32,
}

impl FixedDelayRetryStrategy {
    pub fn new(fixed_delay: Duration) -> Self {
        Self {
            fixed_delay,
            max_attempts: 4,
        }
    }

    pub fn one_second_delay() -> Self {
        Self::new(Duration::from_secs(1))
    }
}

impl RetryStrategy for FixedDelayRetryStrategy {
    fn should_attempt_initial_request(&self, _cfg: &ConfigBag) -> Result<ShouldAttempt, BoxError> {
        Ok(ShouldAttempt::Yes)
    }

    fn should_attempt_retry(
        &self,
        ctx: &InterceptorContext,
        cfg: &ConfigBag,
    ) -> Result<ShouldAttempt, BoxError> {
        // Look a the result. If it's OK then we're done; No retry required. Otherwise, we need to inspect it
        let error = match ctx.output_or_error() {
            Ok(_) => {
                tracing::trace!("request succeeded, no retry necessary");
                return Ok(ShouldAttempt::No);
            }
            Err(err) => err,
        };

        let request_attempts: &RequestAttempts = cfg
            .get()
            .expect("at least one request attempt is made before any retry is attempted");
        if request_attempts.attempts() == self.max_attempts {
            tracing::trace!(
                attempts = request_attempts.attempts(),
                max_attempts = self.max_attempts,
                "not retrying because we are out of attempts"
            );
            return Ok(ShouldAttempt::No);
        }

        let retry_classifiers = cfg
            .get::<RetryClassifiers>()
            .expect("a retry classifier is set");
        let retry_reason = retry_classifiers.classify_retry(error);

        let backoff = match retry_reason {
            Some(RetryReason::Explicit(_)) => self.fixed_delay,
            Some(RetryReason::Error(_)) => self.fixed_delay,
            Some(_) => {
                unreachable!("RetryReason is non-exhaustive. Therefore, we need to cover this unreachable case.")
            }
            None => {
                tracing::trace!(
                    attempts = request_attempts.attempts(),
                    max_attempts = self.max_attempts,
                    "encountered unretryable error"
                );
                return Ok(ShouldAttempt::No);
            }
        };

        tracing::debug!(
            "attempt {} failed with {:?}; retrying after {:?}",
            request_attempts.attempts(),
            retry_reason,
            backoff
        );

        Ok(ShouldAttempt::YesAfterDelay(backoff))
    }
}
