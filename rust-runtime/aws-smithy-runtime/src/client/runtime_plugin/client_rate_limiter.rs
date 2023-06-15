/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! A rate limiter for controlling the rate at which AWS requests are made. The rate changes based
//! on the number of throttling errors encountered.

// Zelda will integrate this rate limiter into the retry policy in a separate PR.
#![allow(dead_code)]

use aws_smithy_runtime_api::client::orchestrator::{BoxError, ConfigBagAccessors};
use aws_smithy_runtime_api::client::runtime_plugin::RuntimePlugin;
use aws_smithy_types::config_bag::{ConfigBag, FrozenLayer, Layer, Storable, StoreReplace};
use aws_smithy_types::{builder, builder_methods, builder_struct};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// A [RuntimePlugin] to provide a standard token bucket, usable by the
/// [`StandardRetryStrategy`](crate::client::retries::strategy::standard::StandardRetryStrategy).
#[non_exhaustive]
#[derive(Debug)]
pub struct ClientRateLimiterRuntimePlugin {
    _rate_limiter: Arc<Mutex<ClientRateLimiter>>,
}

impl ClientRateLimiterRuntimePlugin {
    pub fn new(cfg: &ConfigBag) -> Self {
        Self {
            _rate_limiter: Arc::new(Mutex::new(ClientRateLimiter::new(cfg))),
        }
    }
}

impl RuntimePlugin for ClientRateLimiterRuntimePlugin {
    fn config(&self) -> Option<FrozenLayer> {
        let cfg = Layer::new("client rate limiter");
        // TODO(enableNewSmithyRuntimeLaunch) Move the Arc/Mutex inside the rate limiter so that it
        //    be both storable and cloneable.
        // cfg.store_put(self.rate_limiter.clone());

        Some(cfg.freeze())
    }
}

const MIN_FILL_RATE: f64 = 0.5;
const MIN_CAPACITY: f64 = 1.0;
const SMOOTH: f64 = 0.8;
/// How much to scale back after receiving a throttling response
const BETA: f64 = 0.7;
/// Controls how aggressively we scale up after being throttled
const SCALE_CONSTANT: f64 = 0.4;

#[derive(Clone, Debug)]
pub(crate) struct ClientRateLimiter {
    /// The rate at which token are replenished.
    token_refill_rate: f64,
    /// The maximum capacity allowed in the token bucket.
    maximum_bucket_capacity: f64,
    /// The current capacity of the token bucket.
    /// The minimum this can be is 1.0
    current_bucket_capacity: f64,
    /// The last time the token bucket was refilled.
    time_of_last_refill: Option<f64>,
    /// The smoothed rate which tokens are being retrieved.
    tokens_retrieved_per_second: f64,
    /// The last half second time bucket used.
    previous_time_bucket: f64,
    /// The number of requests seen within the current time bucket.
    request_count: u64,
    /// Boolean indicating if the token bucket is enabled.
    /// The token bucket is initially disabled.
    /// When a throttling error is encountered it is enabled.
    enable_throttling: bool,
    /// The maximum rate when the client was last throttled.
    tokens_retrieved_per_second_at_time_of_last_throttle: f64,
    /// The last time when the client was throttled.
    time_of_last_throttle: f64,
    time_window: f64,
    calculated_rate: f64,
}

impl Storable for ClientRateLimiter {
    type Storer = StoreReplace<Self>;
}

impl ClientRateLimiter {
    pub(crate) fn new(cfg: &ConfigBag) -> Self {
        Self::builder()
            .time_of_last_throttle(get_unix_timestamp(cfg))
            .previous_time_bucket(get_unix_timestamp(cfg).floor())
            .build()
    }

    fn builder() -> Builder {
        Builder::new()
    }

    /// If this function returns `Ok(())`, you're OK to send a request. If it returns an error,
    /// then you should not send a request; You've sent quite enough already.
    pub(crate) fn acquire_permission_to_send_a_request(
        &mut self,
        cfg: &ConfigBag,
        amount: f64,
    ) -> Result<(), BoxError> {
        if !self.enable_throttling {
            // return early if we haven't encountered a throttling error yet
            return Ok(());
        }

        let timestamp = get_unix_timestamp(cfg);
        self.refill(timestamp);

        if self.current_bucket_capacity < amount {
            Err(BoxError::from("the client rate limiter is out of tokens"))
        } else {
            self.current_bucket_capacity -= amount;
            Ok(())
        }
    }

    pub(crate) fn update_rate_limiter(&mut self, cfg: &ConfigBag, is_throttling_error: bool) {
        let timestamp = get_unix_timestamp(cfg);
        self.update_tokens_retrieved_per_second(timestamp);

        if is_throttling_error {
            let rate_to_use = if self.enable_throttling {
                f64::min(self.tokens_retrieved_per_second, self.token_refill_rate)
            } else {
                self.tokens_retrieved_per_second
            };

            // The fill_rate is from the token bucket
            self.tokens_retrieved_per_second_at_time_of_last_throttle = rate_to_use;
            self.calculate_time_window();
            self.time_of_last_throttle = timestamp;
            self.calculated_rate = cubic_throttle(rate_to_use);
            self.enable_token_bucket();
        } else {
            self.calculate_time_window();
            self.calculated_rate = self.cubic_success(timestamp);
        }

        let new_rate = f64::min(self.calculated_rate, 2.0 * self.tokens_retrieved_per_second);
        self.update_bucket_refill_rate(timestamp, new_rate);
    }

    fn refill(&mut self, timestamp: f64) {
        if let Some(last_timestamp) = self.time_of_last_refill {
            let fill_amount = (timestamp - last_timestamp) * self.token_refill_rate;
            self.current_bucket_capacity = f64::min(
                self.maximum_bucket_capacity,
                self.current_bucket_capacity + fill_amount,
            );
        }
        self.time_of_last_refill = Some(timestamp);
    }

    fn update_bucket_refill_rate(&mut self, timestamp: f64, new_fill_rate: f64) {
        // Refill based on our current rate before we update to the new fill rate.
        self.refill(timestamp);

        self.token_refill_rate = f64::max(new_fill_rate, MIN_FILL_RATE);
        self.maximum_bucket_capacity = f64::max(new_fill_rate, MIN_CAPACITY);
        // When we scale down we can't have a current capacity that exceeds our max_capacity.
        self.current_bucket_capacity =
            f64::min(self.current_bucket_capacity, self.maximum_bucket_capacity);
    }

    fn enable_token_bucket(&mut self) {
        self.enable_throttling = true;
    }

    fn update_tokens_retrieved_per_second(&mut self, timestamp: f64) {
        let next_time_bucket = (timestamp * 2.0).floor() / 2.0;
        self.request_count += 1;

        if next_time_bucket > self.previous_time_bucket {
            let current_rate =
                self.request_count as f64 / (next_time_bucket - self.previous_time_bucket);
            self.tokens_retrieved_per_second =
                current_rate * SMOOTH + self.tokens_retrieved_per_second * (1.0 - SMOOTH);
            self.request_count = 0;
            self.previous_time_bucket = next_time_bucket;
        }
    }

    fn calculate_time_window(&mut self) {
        // This is broken out into a separate calculation because it only
        // gets updated when @tokens_retrieved_per_second_at_time_of_last_throttle() changes so it can be cached.
        let base = (self.tokens_retrieved_per_second_at_time_of_last_throttle * (1.0 - BETA))
            / SCALE_CONSTANT;
        self.time_window = base.powf(1.0 / 3.0);
    }

    fn cubic_success(&self, timestamp: f64) -> f64 {
        let dt = timestamp - self.time_of_last_throttle - self.time_window;
        (SCALE_CONSTANT * dt.powi(3)) + self.tokens_retrieved_per_second_at_time_of_last_throttle
    }
}

fn cubic_throttle(rate_to_use: f64) -> f64 {
    rate_to_use * BETA
}

fn get_unix_timestamp(cfg: &ConfigBag) -> f64 {
    let request_time = cfg.request_time().unwrap();
    request_time
        .now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

builder!(
    set_token_refill_rate, token_refill_rate, f64, "The rate at which token are replenished.",
    set_maximum_bucket_capacity, maximum_bucket_capacity, f64, "The maximum capacity allowed in the token bucket.",
    set_current_bucket_capacity, current_bucket_capacity, f64, "The current capacity of the token bucket. The minimum this can be is 1.0",
    set_time_of_last_refill, time_of_last_refill, f64, "The last time the token bucket was refilled.",
    set_tokens_retrieved_per_second, tokens_retrieved_per_second, f64, "The smoothed rate which tokens are being retrieved.",
    set_previous_time_bucket, previous_time_bucket, f64, "The last half second time bucket used.",
    set_request_count, request_count, u64, "The number of requests seen within the current time bucket.",
    set_enable_throttling, enable_throttling, bool, "Boolean indicating if the token bucket is enabled. The token bucket is initially disabled. When a throttling error is encountered it is enabled.",
    set_tokens_retrieved_per_second_at_time_of_last_throttle, tokens_retrieved_per_second_at_time_of_last_throttle, f64, "The maximum rate when the client was last throttled.",
    set_time_of_last_throttle, time_of_last_throttle, f64, "The last time when the client was throttled.",
    set_time_window, time_window, f64, "The time window used to calculate the cubic success rate.",
    set_calculated_rate, calculated_rate, f64, "The calculated rate used to update the sending rate."
);

impl Builder {
    fn build(self) -> ClientRateLimiter {
        ClientRateLimiter {
            token_refill_rate: self.token_refill_rate.unwrap_or_default(),
            maximum_bucket_capacity: self.maximum_bucket_capacity.unwrap_or(f64::MAX),
            current_bucket_capacity: self.current_bucket_capacity.unwrap_or_default(),
            time_of_last_refill: self.time_of_last_refill,
            enable_throttling: self.enable_throttling.unwrap_or_default(),
            tokens_retrieved_per_second: self.tokens_retrieved_per_second.unwrap_or_default(),
            previous_time_bucket: self.previous_time_bucket.unwrap_or_default(),
            request_count: self.request_count.unwrap_or_default(),
            tokens_retrieved_per_second_at_time_of_last_throttle: self
                .tokens_retrieved_per_second_at_time_of_last_throttle
                .unwrap_or_default(),
            time_of_last_throttle: self.time_of_last_throttle.unwrap_or_default(),
            time_window: self.time_window.unwrap_or_default(),
            calculated_rate: self.calculated_rate.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_unix_timestamp, ClientRateLimiter};
    use crate::client::runtime_plugin::client_rate_limiter::cubic_throttle;
    use approx::assert_relative_eq;
    use aws_smithy_async::rt::sleep::{AsyncSleep, SharedAsyncSleep};
    use aws_smithy_async::test_util::instant_time_and_sleep;
    use aws_smithy_async::time::SharedTimeSource;
    use aws_smithy_runtime_api::client::orchestrator::ConfigBagAccessors;
    use aws_smithy_types::config_bag::ConfigBag;
    use std::time::{Duration, SystemTime};

    #[test]
    fn it_sets_the_time_window_correctly() {
        let mut rate_limiter = ClientRateLimiter::builder()
            .tokens_retrieved_per_second_at_time_of_last_throttle(10.0)
            .build();

        rate_limiter.calculate_time_window();
        assert_relative_eq!(rate_limiter.time_window, 1.9574338205844317);
    }

    #[test]
    fn should_match_beta_decrease() {
        let new_rate = cubic_throttle(10.0);
        assert_relative_eq!(new_rate, 7.0);

        let mut rate_limiter = ClientRateLimiter::builder()
            .tokens_retrieved_per_second_at_time_of_last_throttle(10.0)
            .time_of_last_throttle(1.0)
            .build();

        rate_limiter.calculate_time_window();
        let new_rate = rate_limiter.cubic_success(1.0);
        assert_relative_eq!(new_rate, 7.0);
    }

    #[tokio::test]
    async fn throttling_is_enabled_once_throttling_error_is_received() {
        let mut cfg = ConfigBag::base();
        let (time_source, sleep_impl) = instant_time_and_sleep(SystemTime::UNIX_EPOCH);
        cfg.interceptor_state()
            .set_request_time(SharedTimeSource::new(time_source));
        cfg.interceptor_state()
            .set_sleep_impl(Some(SharedAsyncSleep::new(sleep_impl)));
        let now = get_unix_timestamp(&cfg);
        let mut rate_limiter = ClientRateLimiter::builder()
            .previous_time_bucket((now).floor())
            .time_of_last_throttle(now)
            .build();

        assert!(
            !rate_limiter.enable_throttling,
            "rate_limiter should be disabled by default"
        );
        rate_limiter.update_rate_limiter(&cfg, true);
        assert!(
            rate_limiter.enable_throttling,
            "rate_limiter should be enabled after throttling error"
        );
    }

    #[tokio::test]
    async fn test_calculated_rate_with_successes() {
        let mut cfg = ConfigBag::base();
        let (time_source, sleep_impl) = instant_time_and_sleep(SystemTime::UNIX_EPOCH);
        sleep_impl.sleep(Duration::from_secs(5)).await;
        cfg.interceptor_state()
            .set_request_time(SharedTimeSource::new(time_source));
        cfg.interceptor_state()
            .set_sleep_impl(Some(SharedAsyncSleep::new(sleep_impl.clone())));
        let now = get_unix_timestamp(&cfg);
        let mut rate_limiter = ClientRateLimiter::builder()
            .time_of_last_throttle(now)
            .tokens_retrieved_per_second_at_time_of_last_throttle(10.0)
            .build();

        struct Attempt {
            time_since_start: Duration,
            expected_calculated_rate: f64,
        }

        let attempts = [
            Attempt {
                time_since_start: Duration::from_secs(5),
                expected_calculated_rate: 7.0,
            },
            Attempt {
                time_since_start: Duration::from_secs(6),
                expected_calculated_rate: 9.64893600966,
            },
            Attempt {
                time_since_start: Duration::from_secs(7),
                expected_calculated_rate: 10.000030849917364,
            },
            Attempt {
                time_since_start: Duration::from_secs(8),
                expected_calculated_rate: 10.453284520772092,
            },
            Attempt {
                time_since_start: Duration::from_secs(9),
                expected_calculated_rate: 13.408697022224185,
            },
            Attempt {
                time_since_start: Duration::from_secs(10),
                expected_calculated_rate: 21.26626835427364,
            },
            Attempt {
                time_since_start: Duration::from_secs(11),
                expected_calculated_rate: 36.425998516920465,
            },
        ];

        // Think this test is a little strange? I ported the test from Go v2, and this is how it
        // was implemented. See for yourself:
        // https://github.com/aws/aws-sdk-go-v2/blob/844ff45cdc76182229ad098c95bf3f5ab8c20e9f/aws/retry/adaptive_ratelimit_test.go#L97
        for attempt in attempts {
            rate_limiter.calculate_time_window();
            let calculated_rate =
                rate_limiter.cubic_success(attempt.time_since_start.as_secs_f64());

            assert_relative_eq!(attempt.expected_calculated_rate, calculated_rate);
        }
    }

    #[tokio::test]
    async fn test_calculated_rate_with_throttles() {
        let mut cfg = ConfigBag::base();
        let (time_source, sleep_impl) = instant_time_and_sleep(SystemTime::UNIX_EPOCH);
        sleep_impl.sleep(Duration::from_secs(5)).await;
        cfg.interceptor_state()
            .set_request_time(SharedTimeSource::new(time_source));
        cfg.interceptor_state()
            .set_sleep_impl(Some(SharedAsyncSleep::new(sleep_impl.clone())));
        let now = get_unix_timestamp(&cfg);
        let mut rate_limiter = ClientRateLimiter::builder()
            .tokens_retrieved_per_second_at_time_of_last_throttle(10.0)
            .time_of_last_throttle(now)
            .build();

        struct Attempt {
            throttled: bool,
            time_since_start: Duration,
            expected_calculated_rate: f64,
        }

        let attempts = [
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs(5),
                expected_calculated_rate: 7.0,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs(6),
                expected_calculated_rate: 9.64893600966,
            },
            Attempt {
                throttled: true,
                time_since_start: Duration::from_secs(7),
                expected_calculated_rate: 6.754255206761999,
            },
            Attempt {
                throttled: true,
                time_since_start: Duration::from_secs(8),
                expected_calculated_rate: 4.727978644733399,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs(9),
                expected_calculated_rate: 4.670125557970046,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs(10),
                expected_calculated_rate: 4.770870456867401,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs(11),
                expected_calculated_rate: 6.011819748005445,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs(12),
                expected_calculated_rate: 10.792973431384178,
            },
        ];

        // Think this test is a little strange? I ported the test from Go v2, and this is how it
        // was implemented. See for yourself:
        // https://github.com/aws/aws-sdk-go-v2/blob/844ff45cdc76182229ad098c95bf3f5ab8c20e9f/aws/retry/adaptive_ratelimit_test.go#L97
        let mut calculated_rate = 0.0;
        for attempt in attempts {
            rate_limiter.calculate_time_window();
            if attempt.throttled {
                calculated_rate = cubic_throttle(calculated_rate);
                rate_limiter.time_of_last_throttle = attempt.time_since_start.as_secs_f64();
                rate_limiter.tokens_retrieved_per_second_at_time_of_last_throttle = calculated_rate;
            } else {
                calculated_rate =
                    rate_limiter.cubic_success(attempt.time_since_start.as_secs_f64());
            };

            assert_relative_eq!(attempt.expected_calculated_rate, calculated_rate);
        }
    }

    #[tokio::test]
    async fn test_client_sending_rates() {
        let mut cfg = ConfigBag::base();
        let (time_source, sleep_impl) = instant_time_and_sleep(SystemTime::UNIX_EPOCH);
        cfg.interceptor_state()
            .set_request_time(SharedTimeSource::new(time_source));
        cfg.interceptor_state()
            .set_sleep_impl(Some(SharedAsyncSleep::new(sleep_impl.clone())));
        let mut rate_limiter = ClientRateLimiter::builder().build();

        struct Attempt {
            throttled: bool,
            time_since_start: Duration,
            expected_tokens_retrieved_per_second: f64,
            expected_token_refill_rate: f64,
        }

        let attempts = [
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(0.2),
                expected_tokens_retrieved_per_second: 0.000000,
                expected_token_refill_rate: 0.500000,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(0.4),
                expected_tokens_retrieved_per_second: 0.000000,
                expected_token_refill_rate: 0.500000,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(0.6),
                expected_tokens_retrieved_per_second: 4.800000000000001,
                expected_token_refill_rate: 0.500000,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(0.8),
                expected_tokens_retrieved_per_second: 4.800000000000001,
                expected_token_refill_rate: 0.500000,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(1.0),
                expected_tokens_retrieved_per_second: 4.160000,
                expected_token_refill_rate: 0.500000,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(1.2),
                expected_tokens_retrieved_per_second: 4.160000,
                expected_token_refill_rate: 0.691200,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(1.4),
                expected_tokens_retrieved_per_second: 4.160000,
                expected_token_refill_rate: 1.0975999999999997,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(1.6),
                expected_tokens_retrieved_per_second: 5.632000000000001,
                expected_token_refill_rate: 1.6384000000000005,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(1.8),
                expected_tokens_retrieved_per_second: 5.632000000000001,
                expected_token_refill_rate: 2.332800,
            },
            Attempt {
                throttled: true,
                time_since_start: Duration::from_secs_f64(2.0),
                expected_tokens_retrieved_per_second: 4.326400,
                expected_token_refill_rate: 3.0284799999999996,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(2.2),
                expected_tokens_retrieved_per_second: 4.326400,
                expected_token_refill_rate: 3.48663917347026,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(2.4),
                expected_tokens_retrieved_per_second: 4.326400,
                expected_token_refill_rate: 3.821874416040255,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(2.6),
                expected_tokens_retrieved_per_second: 5.665280,
                expected_token_refill_rate: 4.053385727709987,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(2.8),
                expected_tokens_retrieved_per_second: 5.665280,
                expected_token_refill_rate: 4.200373108479454,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(3.0),
                expected_tokens_retrieved_per_second: 4.333056,
                expected_token_refill_rate: 4.282036558348658,
            },
            Attempt {
                throttled: true,
                time_since_start: Duration::from_secs_f64(3.2),
                expected_tokens_retrieved_per_second: 4.333056,
                expected_token_refill_rate: 2.99742559084406,
            },
            Attempt {
                throttled: false,
                time_since_start: Duration::from_secs_f64(3.4),
                expected_tokens_retrieved_per_second: 4.333056,
                expected_token_refill_rate: 3.4522263943863463,
            },
        ];

        let two_hundred_milliseconds = Duration::from_millis(200);
        for (i, attempt) in attempts.into_iter().enumerate() {
            sleep_impl.sleep(two_hundred_milliseconds).await;
            assert_eq!(attempt.time_since_start, sleep_impl.total_duration());

            rate_limiter.update_rate_limiter(&cfg, attempt.throttled);
            assert_relative_eq!(
                attempt.expected_tokens_retrieved_per_second,
                rate_limiter.tokens_retrieved_per_second
            );
            assert_relative_eq!(
                attempt.expected_token_refill_rate,
                rate_limiter.token_refill_rate
            );
        }
    }
}
