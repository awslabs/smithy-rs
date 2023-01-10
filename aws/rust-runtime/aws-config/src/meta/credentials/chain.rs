/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_credential_types::provider::{self, error::CredentialsError, future, ProvideCredentials};
use aws_smithy_async::future::timeout::Timeout;
use aws_smithy_async::rt::sleep::AsyncSleep;
use aws_smithy_types::error::display::DisplayErrorContext;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use tracing::Instrument;

/// Credentials provider that checks a series of inner providers
///
/// Each provider will be evaluated in order:
/// * If a provider returns valid [`Credentials`](aws_credential_types::Credentials) they will be returned immediately.
///   No other credential providers will be used.
/// * Otherwise, if a provider returns
///   [`CredentialsError::CredentialsNotLoaded`](aws_credential_types::provider::error::CredentialsError::CredentialsNotLoaded),
///   the next provider will be checked.
/// * Finally, if a provider returns any other error condition, an error will be returned immediately.
///
/// # Examples
///
/// ```no_run
/// # fn example() {
/// use aws_config::meta::credentials::CredentialsProviderChain;
/// use aws_config::environment::credentials::EnvironmentVariableCredentialsProvider;
/// use aws_config::profile::ProfileFileCredentialsProvider;
///
/// let provider = CredentialsProviderChain::first_try("Environment", EnvironmentVariableCredentialsProvider::new())
///     .or_else("Profile", ProfileFileCredentialsProvider::builder().build());
/// # }
/// ```
#[derive(Debug)]
pub struct CredentialsProviderChain {
    providers: Vec<(Cow<'static, str>, Box<dyn ProvideCredentials>)>,
}

impl CredentialsProviderChain {
    /// Create a `CredentialsProviderChain` that begins by evaluating this provider
    pub fn first_try(
        name: impl Into<Cow<'static, str>>,
        provider: impl ProvideCredentials + 'static,
    ) -> Self {
        CredentialsProviderChain {
            providers: vec![(name.into(), Box::new(provider))],
        }
    }

    /// Add a fallback provider to the credentials provider chain
    pub fn or_else(
        mut self,
        name: impl Into<Cow<'static, str>>,
        provider: impl ProvideCredentials + 'static,
    ) -> Self {
        self.providers.push((name.into(), Box::new(provider)));
        self
    }

    /// Add a fallback to the default provider chain
    #[cfg(any(feature = "rustls", feature = "native-tls"))]
    pub async fn or_default_provider(self) -> Self {
        self.or_else(
            "DefaultProviderChain",
            crate::default_provider::credentials::default_provider().await,
        )
    }

    /// Creates a credential provider chain that starts with the default provider
    #[cfg(any(feature = "rustls", feature = "native-tls"))]
    pub async fn default_provider() -> Self {
        Self::first_try(
            "DefaultProviderChain",
            crate::default_provider::credentials::default_provider().await,
        )
    }

    async fn credentials(&self) -> provider::Result {
        for (name, provider) in &self.providers {
            let span = tracing::debug_span!("load_credentials", provider = %name);
            match provider.provide_credentials().instrument(span).await {
                Ok(credentials) => {
                    tracing::debug!(provider = %name, "loaded credentials");
                    return Ok(credentials);
                }
                Err(err @ CredentialsError::CredentialsNotLoaded(_)) => {
                    tracing::debug!(provider = %name, context = %DisplayErrorContext(&err), "provider in chain did not provide credentials");
                }
                Err(err) => {
                    tracing::warn!(provider = %name, error = %DisplayErrorContext(&err), "provider failed to provide credentials");
                    return Err(err);
                }
            }
        }
        Err(CredentialsError::not_loaded(
            "no providers in chain provided credentials",
        ))
    }

    async fn credentials_with_timeout(
        &self,
        sleeper: Arc<dyn AsyncSleep>,
        timeout: Duration,
    ) -> provider::Result {
        for (name, provider) in &self.providers {
            let span = tracing::debug_span!("load_credentials", provider = %name);
            match provider
                .provide_credentials_with_timeout(Arc::clone(&sleeper), timeout)
                .instrument(span)
                .await
            {
                Ok(credentials) => {
                    tracing::debug!(provider = %name, "loaded credentials");
                    return Ok(credentials);
                }
                Err(err @ CredentialsError::ProviderTimedOut(_)) => {
                    tracing::debug!(provider = %name, context = %DisplayErrorContext(&err), "provider in chain did not provide credentials");
                }
                Err(err) => {
                    tracing::warn!(provider = %name, error = %DisplayErrorContext(&err), "provider failed to provide credentials");
                    return Err(err);
                }
            }
        }
        Err(CredentialsError::provider_timed_out(timeout))
    }
}

impl ProvideCredentials for CredentialsProviderChain {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'_>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.credentials())
    }

    fn provide_credentials_with_timeout<'a>(
        &'a self,
        sleeper: Arc<dyn AsyncSleep>,
        timeout: Duration,
    ) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        // We need to give `timeout` to the whole chain as well as to each credentials provider in the chain.
        // One could argue that if the whole chain has `timeout` anyway, there is no point in having it
        // in individual providers. This is due to the fact that the other method `provide_credentials`
        // does not respect provider-specific read timeout behavior, e.g. the IMDS credentials provider
        // wants to provide expired credentials, if any, in the case of read timeout.
        let sleep_future = sleeper.sleep(timeout);
        let timeout_future = Timeout::new(
            self.credentials_with_timeout(sleeper, timeout),
            sleep_future,
        );
        future::ProvideCredentials::new(async move {
            match timeout_future.await {
                Ok(creds) => creds,
                Err(_) => Err(CredentialsError::provider_timed_out(timeout)),
            }
        })
    }
}
