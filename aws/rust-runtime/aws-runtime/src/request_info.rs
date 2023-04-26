/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_smithy_runtime::client::orchestrator::interceptors::{RequestAttempts, ServiceClockSkew};
use aws_smithy_runtime_api::client::interceptors::{BoxError, Interceptor, InterceptorContext};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
use aws_smithy_runtime_api::config_bag::ConfigBag;
use aws_smithy_types::date_time::Format;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;
use aws_smithy_types::DateTime;
use http::{HeaderName, HeaderValue};
use std::borrow::Cow;
use std::time::{Duration, SystemTime};

#[allow(clippy::declare_interior_mutable_const)] // we will never mutate this
const AMZ_SDK_REQUEST: HeaderName = HeaderName::from_static("amz-sdk-request");

/// Generates and attaches a request header that communicates request-related metadata.
/// Examples include:
///
/// - When the client will time out this request.
/// - How many times the request has been retried.
/// - The maximum number of retries that the client will attempt.
#[non_exhaustive]
#[derive(Debug, Default)]
pub struct RequestInfoInterceptor {}

impl RequestInfoInterceptor {
    /// Creates a new `RequestInfoInterceptor`
    pub fn new() -> Self {
        RequestInfoInterceptor {}
    }
}

impl RequestInfoInterceptor {
    fn build_attempts_pair(
        &self,
        cfg: &ConfigBag,
    ) -> Option<(Cow<'static, str>, Cow<'static, str>)> {
        let request_attempts = cfg
            .get::<RequestAttempts>()
            .map(|r_a| r_a.attempts())
            .unwrap_or(1);
        let request_attempts = request_attempts.to_string();
        Some((Cow::Borrowed("attempt"), Cow::Owned(request_attempts)))
    }

    fn build_max_attempts_pair(
        &self,
        cfg: &ConfigBag,
    ) -> Option<(Cow<'static, str>, Cow<'static, str>)> {
        // TODO(orchestrator_retries) What config will we actually store in the bag? Will it be a whole config or just the max_attempts part?
        if let Some(retry_config) = cfg.get::<RetryConfig>() {
            let max_attempts = retry_config.max_attempts().to_string();
            Some((Cow::Borrowed("max"), Cow::Owned(max_attempts)))
        } else {
            None
        }
    }

    fn build_ttl_pair(&self, cfg: &ConfigBag) -> Option<(Cow<'static, str>, Cow<'static, str>)> {
        let timeout_config = cfg.get::<TimeoutConfig>()?;
        let socket_read = timeout_config.read_timeout()?;
        let estimated_skew: Duration = cfg.get::<ServiceClockSkew>().cloned()?.into();
        let current_time = SystemTime::now();
        let ttl = current_time.checked_add(socket_read + estimated_skew)?;
        let timestamp = DateTime::from(ttl);
        let formatted_timestamp = timestamp
            .fmt(Format::DateTime)
            .expect("the resulting DateTime will always be valid");

        Some((Cow::Borrowed("ttl"), Cow::Owned(formatted_timestamp)))
    }
}

impl Interceptor<HttpRequest, HttpResponse> for RequestInfoInterceptor {
    fn modify_before_transmit(
        &self,
        context: &mut InterceptorContext<HttpRequest, HttpResponse>,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let mut pairs = RequestPairs::new();
        if let Some(pair) = self.build_attempts_pair(cfg) {
            pairs = pairs.with_pair(pair);
        }
        if let Some(pair) = self.build_max_attempts_pair(cfg) {
            pairs = pairs.with_pair(pair);
        }
        if let Some(pair) = self.build_ttl_pair(cfg) {
            pairs = pairs.with_pair(pair);
        }

        let headers = context.request_mut()?.headers_mut();
        headers.insert(AMZ_SDK_REQUEST, pairs.try_into_header_value()?);

        Ok(())
    }
}

/// A builder for creating a `RequestPairs` header value. `RequestPairs` is used to generate a
/// retry information header that is sent with every request. The information conveyed by this
/// header allows services to anticipate whether a client will time out or retry a request.
#[derive(Default, Debug)]
pub struct RequestPairs {
    inner: Vec<(Cow<'static, str>, Cow<'static, str>)>,
}

impl RequestPairs {
    /// Creates a new `RequestPairs` builder.
    pub fn new() -> Self {
        Default::default()
    }

    /// Adds a pair to the `RequestPairs` builder.
    /// Only strings that can be converted to header values are considered valid.
    pub fn with_pair(
        mut self,
        pair: (impl Into<Cow<'static, str>>, impl Into<Cow<'static, str>>),
    ) -> Self {
        let pair = (pair.0.into(), pair.1.into());
        self.inner.push(pair);
        self
    }

    /// Converts the `RequestPairs` builder into a `HeaderValue`.
    pub fn try_into_header_value(self) -> Result<HeaderValue, BoxError> {
        self.try_into()
    }
}

impl TryFrom<RequestPairs> for HeaderValue {
    type Error = BoxError;

    fn try_from(value: RequestPairs) -> Result<Self, BoxError> {
        let mut pairs = String::new();
        for (key, value) in value.inner {
            if !pairs.is_empty() {
                pairs.push_str("; ");
            }

            // TODO Do I need to escape/encode these?
            pairs.push_str(&key);
            pairs.push('=');
            pairs.push_str(&value);
            continue;
        }
        HeaderValue::from_str(&pairs).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::RequestInfoInterceptor;
    use aws_smithy_http::body::SdkBody;
    use aws_smithy_runtime::client::orchestrator::interceptors::RequestAttempts;
    use aws_smithy_runtime_api::client::interceptors::{Interceptor, InterceptorContext};
    use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
    use aws_smithy_runtime_api::config_bag::ConfigBag;
    use aws_smithy_runtime_api::type_erasure::TypedBox;
    use aws_smithy_types::retry::RetryConfig;
    use aws_smithy_types::timeout::TimeoutConfig;
    use std::time::Duration;

    fn expect_header<'a>(
        context: &'a InterceptorContext<HttpRequest, HttpResponse>,
        header_name: &str,
    ) -> &'a str {
        context
            .request()
            .unwrap()
            .headers()
            .get(header_name)
            .unwrap()
            .to_str()
            .unwrap()
    }

    #[test]
    fn test_request_pairs_for_initial_attempt() {
        let mut context = InterceptorContext::new(TypedBox::new("doesntmatter").erase());
        context.set_request(http::Request::builder().body(SdkBody::empty()).unwrap());

        let mut config = ConfigBag::base();
        config.put(RetryConfig::standard());
        config.put(
            TimeoutConfig::builder()
                .read_timeout(Duration::from_secs(30))
                .build(),
        );
        config.put(RequestAttempts::new());

        let interceptor = RequestInfoInterceptor::new();
        interceptor
            .modify_before_transmit(&mut context, &mut config)
            .unwrap();

        assert_eq!(
            expect_header(&context, "amz-sdk-request"),
            "attempt=0; max=3"
        );
    }
}
