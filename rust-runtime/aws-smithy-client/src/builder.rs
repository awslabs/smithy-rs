/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::{bounds, erase, retry, Client};
use aws_smithy_async::rt::sleep::{default_async_sleep, AsyncSleep};
use aws_smithy_http::body::SdkBody;
use aws_smithy_http::result::ConnectorError;
use aws_smithy_types::timeout::TimeoutConfig;
use std::sync::Arc;

/// A builder that provides more customization options when constructing a [`Client`].
///
/// To start, call [`Builder::new`]. Then, chain the method calls to configure the `Builder`.
/// When configured to your liking, call [`Builder::build`]. The individual methods have additional
/// documentation.
#[derive(Clone, Debug)]
pub struct Builder<C = (), M = (), R = retry::Standard> {
    connector: C,
    middleware: M,
    // Keep a copy of standard retry config when the standard policy is used
    // so that we can do additional validation against the `sleep_impl` when
    // the client.
    standard_retry_config: Option<retry::Config>,
    retry_policy: R,
    timeout_config: TimeoutConfig,
    sleep_impl: Option<Arc<dyn AsyncSleep>>,
}

impl<C, M> Default for Builder<C, M>
where
    C: Default,
    M: Default,
{
    fn default() -> Self {
        Self {
            connector: Default::default(),
            middleware: Default::default(),
            standard_retry_config: Some(retry::Config::default()),
            retry_policy: Default::default(),
            timeout_config: TimeoutConfig::disabled(),
            sleep_impl: default_async_sleep(),
        }
    }
}

// It'd be nice to include R where R: Default here, but then the caller ends up always having to
// specify R explicitly since type parameter defaults (like the one for R) aren't picked up when R
// cannot be inferred. This is, arguably, a compiler bug/missing language feature, but is
// complicated: https://github.com/rust-lang/rust/issues/27336.
//
// For the time being, we stick with just <C, M> for ::new. Those can usually be inferred since we
// only implement .constructor and .middleware when C and M are () respectively. Users who really
// need a builder for a custom R can use ::default instead.
impl<C, M> Builder<C, M>
where
    C: Default,
    M: Default,
{
    /// Construct a new builder. This does not specify a [connector](Builder::connector)
    /// or [middleware](Builder::middleware).
    /// It uses the [standard retry mechanism](retry::Standard).
    pub fn new() -> Self {
        Self::default()
    }
}

impl<M, R> Builder<(), M, R> {
    /// Specify the connector for the eventual client to use.
    ///
    /// The connector dictates how requests are turned into responses. Normally, this would entail
    /// sending the request to some kind of remote server, but in certain settings it's useful to
    /// be able to use a custom connector instead, such as to mock the network for tests.
    ///
    /// If you just want to specify a function from request to response instead, use
    /// [`Builder::connector_fn`].
    pub fn connector<C>(self, connector: C) -> Builder<C, M, R> {
        Builder {
            connector,
            standard_retry_config: self.standard_retry_config,
            retry_policy: self.retry_policy,
            middleware: self.middleware,
            timeout_config: self.timeout_config,
            sleep_impl: self.sleep_impl,
        }
    }

    /// Use a function that directly maps each request to a response as a connector.
    ///
    /// ```no_run
    /// use aws_smithy_client::Builder;
    /// use aws_smithy_http::body::SdkBody;
    /// let client = Builder::new()
    /// # /*
    ///   .middleware(..)
    /// # */
    /// # .middleware(tower::layer::util::Identity::new())
    ///   .connector_fn(|req: http::Request<SdkBody>| {
    ///     async move {
    ///       Ok(http::Response::new(SdkBody::empty()))
    ///     }
    ///   })
    ///   .build();
    /// # client.check();
    /// ```
    pub fn connector_fn<F, FF>(self, map: F) -> Builder<tower::util::ServiceFn<F>, M, R>
    where
        F: Fn(http::Request<SdkBody>) -> FF + Send,
        FF: std::future::Future<Output = Result<http::Response<SdkBody>, ConnectorError>>,
        // NOTE: The extra bound here is to help the type checker give better errors earlier.
        tower::util::ServiceFn<F>: bounds::SmithyConnector,
    {
        self.connector(tower::service_fn(map))
    }
}

impl<C, R> Builder<C, (), R> {
    /// Specify the middleware for the eventual client ot use.
    ///
    /// The middleware adjusts requests before they are dispatched to the connector. It is
    /// responsible for filling in any request parameters that aren't specified by the Smithy
    /// protocol definition, such as those used for routing (like the URL), authentication, and
    /// authorization.
    ///
    /// The middleware takes the form of a [`tower::Layer`] that wraps the actual connection for
    /// each request. The [`tower::Service`] that the middleware produces must accept requests of
    /// the type [`aws_smithy_http::operation::Request`] and return responses of the type
    /// [`http::Response<SdkBody>`], most likely by modifying the provided request in place,
    /// passing it to the inner service, and then ultimately returning the inner service's
    /// response.
    ///
    /// If your requests are already ready to be sent and need no adjustment, you can use
    /// [`tower::layer::util::Identity`] as your middleware.
    pub fn middleware<M>(self, middleware: M) -> Builder<C, M, R> {
        Builder {
            connector: self.connector,
            standard_retry_config: self.standard_retry_config,
            retry_policy: self.retry_policy,
            timeout_config: self.timeout_config,
            middleware,
            sleep_impl: self.sleep_impl,
        }
    }

    /// Use a function-like middleware that directly maps each request.
    ///
    /// ```no_run
    /// use aws_smithy_client::Builder;
    /// use aws_smithy_client::erase::DynConnector;
    /// use aws_smithy_client::never::NeverConnector;
    /// use aws_smithy_http::body::SdkBody;
    /// let my_connector = DynConnector::new(
    ///     // Your own connector here or use `dyn_https()`
    ///     # NeverConnector::new()
    /// );
    /// let client = Builder::new()
    ///   .connector(my_connector)
    ///   .middleware_fn(|req: aws_smithy_http::operation::Request| {
    ///     req
    ///   })
    ///   .build();
    /// # client.check();
    /// ```
    pub fn middleware_fn<F>(self, map: F) -> Builder<C, tower::util::MapRequestLayer<F>, R>
    where
        F: Fn(aws_smithy_http::operation::Request) -> aws_smithy_http::operation::Request
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.middleware(tower::util::MapRequestLayer::new(map))
    }
}

impl<C, M> Builder<C, M, retry::Standard> {
    /// Specify the retry policy for the eventual client to use.
    ///
    /// By default, the Smithy client uses a standard retry policy that works well in most
    /// settings. You can use this method to override that policy with a custom one. A new policy
    /// instance will be instantiated for each request using [`retry::NewRequestPolicy`]. Each
    /// policy instance must implement [`tower::retry::Policy`].
    ///
    /// If you just want to modify the policy _configuration_ for the standard retry policy, use
    /// [`Builder::set_retry_config`].
    pub fn retry_policy<R>(self, retry_policy: R) -> Builder<C, M, R> {
        Builder {
            connector: self.connector,
            // Intentionally clear out the standard retry config when the retry policy is overridden.
            standard_retry_config: None,
            retry_policy,
            timeout_config: self.timeout_config,
            middleware: self.middleware,
            sleep_impl: self.sleep_impl,
        }
    }
}

impl<C, M> Builder<C, M> {
    /// Set the standard retry policy's configuration.
    pub fn set_retry_config(&mut self, config: retry::Config) -> &mut Self {
        self.standard_retry_config = Some(config.clone());
        self.retry_policy.with_config(config);
        self
    }

    /// Set the standard retry policy's configuration.
    pub fn retry_config(mut self, config: retry::Config) -> Self {
        self.set_retry_config(config);
        self
    }

    /// Set a timeout config for the builder
    pub fn set_timeout_config(&mut self, timeout_config: TimeoutConfig) -> &mut Self {
        self.timeout_config = timeout_config;
        self
    }

    /// Set a timeout config for the builder
    pub fn timeout_config(mut self, timeout_config: TimeoutConfig) -> Self {
        self.timeout_config = timeout_config;
        self
    }

    /// Set the [`AsyncSleep`] function that the [`Client`] will use to create things like timeout futures.
    pub fn set_sleep_impl(&mut self, async_sleep: Option<Arc<dyn AsyncSleep>>) -> &mut Self {
        self.sleep_impl = async_sleep;
        self
    }

    /// Set the [`AsyncSleep`] function that the [`Client`] will use to create things like timeout futures.
    pub fn sleep_impl(mut self, async_sleep: Option<Arc<dyn AsyncSleep>>) -> Self {
        self.set_sleep_impl(async_sleep);
        self
    }
}

impl<C, M, R> Builder<C, M, R> {
    /// Use a connector that wraps the current connector.
    pub fn map_connector<F, C2>(self, map: F) -> Builder<C2, M, R>
    where
        F: FnOnce(C) -> C2,
    {
        Builder {
            connector: map(self.connector),
            middleware: self.middleware,
            standard_retry_config: self.standard_retry_config,
            retry_policy: self.retry_policy,
            timeout_config: self.timeout_config,
            sleep_impl: self.sleep_impl,
        }
    }

    /// Use a middleware that wraps the current middleware.
    pub fn map_middleware<F, M2>(self, map: F) -> Builder<C, M2, R>
    where
        F: FnOnce(M) -> M2,
    {
        Builder {
            connector: self.connector,
            middleware: map(self.middleware),
            standard_retry_config: self.standard_retry_config,
            retry_policy: self.retry_policy,
            timeout_config: self.timeout_config,
            sleep_impl: self.sleep_impl,
        }
    }

    /// Build a Smithy service [`Client`].
    pub fn build(self) -> Client<C, M, R> {
        if self.sleep_impl.is_none() {
            const ADDITIONAL_HELP: &str =
                "Either disable retry by setting max attempts to one, or pass in a `sleep_impl`. \
                If you're not using Tokio, then an implementation of the `AsyncSleep` trait from \
                the `aws-smithy-async` crate is required for your async runtime. If you are using \
                Tokio, then make sure the `rt-tokio` feature is enabled to have its sleep \
                implementation set automatically.";
            if self
                .standard_retry_config
                .map(|src| src.has_retry())
                .unwrap_or(false)
            {
                panic!("Retries require a `sleep_impl`, but none was passed into the builder. {ADDITIONAL_HELP}");
            } else if self.timeout_config.has_timeouts() {
                panic!("Timeouts require a `sleep_impl`, but none was passed into the builder. {ADDITIONAL_HELP}");
            }
        }

        Client {
            connector: self.connector,
            retry_policy: self.retry_policy,
            middleware: self.middleware,
            timeout_config: self.timeout_config,
            sleep_impl: self.sleep_impl,
        }
    }
}

impl<C, M, R> Builder<C, M, R>
where
    C: bounds::SmithyConnector,
    M: bounds::SmithyMiddleware<erase::DynConnector> + Send + Sync + 'static,
    R: retry::NewRequestPolicy,
{
    /// Build a type-erased Smithy service [`Client`].
    ///
    /// Note that if you're using the standard retry mechanism, [`retry::Standard`], `DynClient<R>`
    /// is equivalent to [`Client`] with no type arguments.
    ///
    /// ```no_run
    /// # #[cfg(feature = "https")]
    /// # fn not_main() {
    /// use aws_smithy_client::{Builder, Client};
    /// struct MyClient {
    ///     client: aws_smithy_client::Client,
    /// }
    ///
    /// let client = Builder::new()
    ///     .https()
    ///     .middleware(tower::layer::util::Identity::new())
    ///     .build_dyn();
    /// let client = MyClient { client };
    /// # client.client.check();
    /// # }
    pub fn build_dyn(self) -> erase::DynClient<R> {
        self.build().into_dyn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::never::NeverConnector;
    use aws_smithy_async::rt::sleep::Sleep;
    use std::panic::{self, AssertUnwindSafe};
    use std::time::Duration;

    #[derive(Clone, Debug)]
    struct StubSleep;
    impl AsyncSleep for StubSleep {
        fn sleep(&self, _duration: Duration) -> Sleep {
            todo!()
        }
    }

    #[test]
    fn defaults_dont_panic() {
        let builder = Builder::new()
            .connector(NeverConnector::new())
            .middleware(tower::layer::util::Identity::new());

        let _ = builder.build();
    }

    #[test]
    fn defaults_panic_if_default_tokio_sleep_not_available() {
        let mut builder = Builder::new()
            .connector(NeverConnector::new())
            .middleware(tower::layer::util::Identity::new());
        builder.set_sleep_impl(None);

        let result = panic::catch_unwind(AssertUnwindSafe(move || {
            let _ = builder.build();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn timeouts_without_sleep_panics() {
        let mut builder = Builder::new()
            .connector(NeverConnector::new())
            .middleware(tower::layer::util::Identity::new());
        builder.set_sleep_impl(None);

        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(Duration::from_secs(1))
            .build();
        assert!(timeout_config.has_timeouts());
        builder.set_timeout_config(timeout_config);

        let result = panic::catch_unwind(AssertUnwindSafe(move || {
            let _ = builder.build();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn retry_without_sleep_panics() {
        let mut builder = Builder::new()
            .connector(NeverConnector::new())
            .middleware(tower::layer::util::Identity::new());
        builder.set_sleep_impl(None);

        let retry_config = retry::Config::default();
        assert!(retry_config.has_retry());
        builder.set_retry_config(retry_config);

        let result = panic::catch_unwind(AssertUnwindSafe(move || {
            let _ = builder.build();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn custom_retry_policy_without_sleep_doesnt_panic() {
        let mut builder = Builder::new()
            .connector(NeverConnector::new())
            .middleware(tower::layer::util::Identity::new())
            // Using standard retry here as a shortcut in the test; someone setting
            // a custom retry policy would manually implement the required traits
            .retry_policy(retry::Standard::default());
        builder.set_sleep_impl(None);
        let _ = builder.build();
    }

    #[test]
    fn no_panics_when_sleep_given() {
        let mut builder = Builder::new()
            .connector(NeverConnector::new())
            .middleware(tower::layer::util::Identity::new());

        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(Duration::from_secs(1))
            .build();
        assert!(timeout_config.has_timeouts());
        builder.set_timeout_config(timeout_config);

        let retry_config = retry::Config::default();
        assert!(retry_config.has_retry());
        builder.set_retry_config(retry_config);

        let _ = builder.build();
    }
}
