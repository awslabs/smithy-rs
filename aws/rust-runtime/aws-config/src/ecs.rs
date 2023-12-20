/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Ecs Credentials Provider
//!
//! This credential provider is frequently used with an AWS-provided credentials service (e.g.
//! [IAM Roles for tasks](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-iam-roles.html)).
//! However, it's possible to use environment variables to configure this provider to use your own
//! credentials sources.
//!
//! This provider is part of the [default credentials chain](crate::default_provider::credentials).
//!
//! ## Configuration
//! **First**: It will check the value of `$AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`. It will use this
//! to construct a URI rooted at `http://169.254.170.2`. For example, if the value of the environment
//! variable was `/credentials`, the SDK would look for credentials at `http://169.254.170.2/credentials`.
//!
//! **Next**: It wil check the value of `$AWS_CONTAINER_CREDENTIALS_FULL_URI`. This specifies the full
//! URL to load credentials. The URL MUST satisfy one of the following two properties:
//! 1. The URL begins with `https`
//! 2. The URL refers to a loopback device. If a URL contains a domain name instead of an IP address,
//! a DNS lookup will be performed. ALL resolved IP addresses MUST refer to a loopback interface, or
//! the credentials provider will return `CredentialsError::InvalidConfiguration`
//!
//! **Finally**: It will check the value of `$AWS_CONTAINER_AUTHORIZATION_TOKEN`. If this is set, the
//! value will be passed in the `Authorization` header.
//!
//! ## Credentials Format
//! Credentials MUST be returned in a JSON format:
//! ```json
//! {
//!    "AccessKeyId" : "MUA...",
//!    "SecretAccessKey" : "/7PC5om....",
//!    "Token" : "AQoDY....=",
//!    "Expiration" : "2016-02-25T06:03:31Z"
//!  }
//! ```
//!
//! Credentials errors MAY be returned with a `code` and `message` field:
//! ```json
//! {
//!   "code": "ErrorCode",
//!   "message": "Helpful error message."
//! }
//! ```

use crate::http_credential_provider::HttpCredentialProvider;
use crate::provider_config::ProviderConfig;
use aws_credential_types::provider::{self, error::CredentialsError, future, ProvideCredentials};
use aws_smithy_http::endpoint::apply_endpoint;
use aws_smithy_runtime_api::client::dns::{ResolveDns, ResolveDnsError, SharedDnsResolver};
use aws_smithy_runtime_api::client::http::HttpConnectorSettings;
use aws_smithy_runtime_api::shared::IntoShared;
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_types::os_shim_internal::Env;
use http::header::InvalidHeaderValue;
use http::uri::{InvalidUri, PathAndQuery, Scheme};
use http::{HeaderValue, Uri};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::net::IpAddr;
use std::time::Duration;
use tokio::sync::OnceCell;

const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

// URL from https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint-v2.html
const BASE_HOST: &str = "http://169.254.170.2";
const ENV_RELATIVE_URI: &str = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";
const ENV_FULL_URI: &str = "AWS_CONTAINER_CREDENTIALS_FULL_URI";
const ENV_AUTHORIZATION: &str = "AWS_CONTAINER_AUTHORIZATION_TOKEN";

/// Credential provider for ECS and generalized HTTP credentials
///
/// See the [module](crate::ecs) documentation for more details.
///
/// This credential provider is part of the default chain.
#[derive(Debug)]
pub struct EcsCredentialsProvider {
    inner: OnceCell<Provider>,
    env: Env,
    builder: Builder,
}

impl EcsCredentialsProvider {
    /// Builder for [`EcsCredentialsProvider`]
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Load credentials from this credentials provider
    pub async fn credentials(&self) -> provider::Result {
        let auth = match self.env.get(ENV_AUTHORIZATION).ok() {
            Some(auth) => Some(HeaderValue::from_str(&auth).map_err(|err| {
                tracing::warn!(token = %auth, "invalid auth token");
                CredentialsError::invalid_configuration(EcsConfigurationError::InvalidAuthToken {
                    err,
                    value: auth,
                })
            })?),
            None => None,
        };
        match self.provider().await {
            Provider::NotConfigured => {
                Err(CredentialsError::not_loaded("ECS provider not configured"))
            }
            Provider::InvalidConfiguration(err) => {
                Err(CredentialsError::invalid_configuration(format!("{}", err)))
            }
            Provider::Configured(provider) => provider.credentials(auth).await,
        }
    }

    async fn provider(&self) -> &Provider {
        self.inner
            .get_or_init(|| Provider::make(self.builder.clone()))
            .await
    }
}

impl ProvideCredentials for EcsCredentialsProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.credentials())
    }
}

/// Inner Provider that can record failed configuration state
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Provider {
    Configured(HttpCredentialProvider),
    NotConfigured,
    InvalidConfiguration(EcsConfigurationError),
}

impl Provider {
    async fn uri(env: Env, dns: Option<SharedDnsResolver>) -> Result<Uri, EcsConfigurationError> {
        let relative_uri = env.get(ENV_RELATIVE_URI).ok();
        let full_uri = env.get(ENV_FULL_URI).ok();
        if let Some(relative_uri) = relative_uri {
            Self::build_full_uri(relative_uri)
        } else if let Some(full_uri) = full_uri {
            let dns = dns.or_else(default_dns);
            validate_full_uri(&full_uri, dns)
                .await
                .map_err(|err| EcsConfigurationError::InvalidFullUri { err, uri: full_uri })
        } else {
            Err(EcsConfigurationError::NotConfigured)
        }
    }

    async fn make(builder: Builder) -> Self {
        let provider_config = builder.provider_config.unwrap_or_default();
        let env = provider_config.env();
        let uri = match Self::uri(env, builder.dns).await {
            Ok(uri) => uri,
            Err(EcsConfigurationError::NotConfigured) => return Provider::NotConfigured,
            Err(err) => return Provider::InvalidConfiguration(err),
        };
        let path = uri.path().to_string();
        let endpoint = {
            let mut parts = uri.into_parts();
            parts.path_and_query = Some(PathAndQuery::from_static("/"));
            Uri::from_parts(parts)
        }
        .expect("parts will be valid")
        .to_string();

        let http_provider = HttpCredentialProvider::builder()
            .configure(&provider_config)
            .http_connector_settings(
                HttpConnectorSettings::builder()
                    .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
                    .read_timeout(DEFAULT_READ_TIMEOUT)
                    .build(),
            )
            .build("EcsContainer", &endpoint, path);
        Provider::Configured(http_provider)
    }

    fn build_full_uri(relative_uri: String) -> Result<Uri, EcsConfigurationError> {
        let mut relative_uri = match relative_uri.parse::<Uri>() {
            Ok(uri) => uri,
            Err(invalid_uri) => {
                tracing::warn!(uri = %DisplayErrorContext(&invalid_uri), "invalid URI loaded from environment");
                return Err(EcsConfigurationError::InvalidRelativeUri {
                    err: invalid_uri,
                    uri: relative_uri,
                });
            }
        };
        let endpoint = Uri::from_static(BASE_HOST);
        apply_endpoint(&mut relative_uri, &endpoint, None)
            .expect("appending relative URLs to the ECS endpoint should always succeed");
        Ok(relative_uri)
    }
}

#[derive(Debug)]
enum EcsConfigurationError {
    InvalidRelativeUri {
        err: InvalidUri,
        uri: String,
    },
    InvalidFullUri {
        err: InvalidFullUriError,
        uri: String,
    },
    InvalidAuthToken {
        err: InvalidHeaderValue,
        value: String,
    },
    NotConfigured,
}

impl Display for EcsConfigurationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EcsConfigurationError::InvalidRelativeUri { err, uri } => write!(
                f,
                "invalid relative URI for ECS provider ({}): {}",
                err, uri
            ),
            EcsConfigurationError::InvalidFullUri { err, uri } => {
                write!(f, "invalid full URI for ECS provider ({}): {}", err, uri)
            }
            EcsConfigurationError::NotConfigured => write!(
                f,
                "No environment variables were set to configure ECS provider"
            ),
            EcsConfigurationError::InvalidAuthToken { err, value } => write!(
                f,
                "`{}` could not be used as a header value for the auth token. {}",
                value, err
            ),
        }
    }
}

impl Error for EcsConfigurationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self {
            EcsConfigurationError::InvalidRelativeUri { err, .. } => Some(err),
            EcsConfigurationError::InvalidFullUri { err, .. } => Some(err),
            EcsConfigurationError::InvalidAuthToken { err, .. } => Some(err),
            EcsConfigurationError::NotConfigured => None,
        }
    }
}

/// Builder for [`EcsCredentialsProvider`]
#[derive(Default, Debug, Clone)]
pub struct Builder {
    provider_config: Option<ProviderConfig>,
    dns: Option<SharedDnsResolver>,
    connect_timeout: Option<Duration>,
    read_timeout: Option<Duration>,
}

impl Builder {
    /// Override the configuration used for this provider
    pub fn configure(mut self, provider_config: &ProviderConfig) -> Self {
        self.provider_config = Some(provider_config.clone());
        self
    }

    /// Override the DNS resolver used to validate URIs
    ///
    /// URIs must refer to loopback addresses. The [`ResolveDns`]
    /// implementation is used to retrieve IP addresses for a given domain.
    pub fn dns(mut self, dns: impl ResolveDns + 'static) -> Self {
        self.dns = Some(dns.into_shared());
        self
    }

    /// Override the connect timeout for the HTTP client
    ///
    /// This value defaults to 2 seconds
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Override the read timeout for the HTTP client
    ///
    /// This value defaults to 5 seconds
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = Some(timeout);
        self
    }

    /// Create an [`EcsCredentialsProvider`] from this builder
    pub fn build(self) -> EcsCredentialsProvider {
        let env = self
            .provider_config
            .as_ref()
            .map(|config| config.env())
            .unwrap_or_default();
        EcsCredentialsProvider {
            inner: OnceCell::new(),
            env,
            builder: self,
        }
    }
}

#[derive(Debug)]
enum InvalidFullUriErrorKind {
    /// The provided URI could not be parsed as a URI
    #[non_exhaustive]
    InvalidUri(InvalidUri),

    /// No Dns resolver was provided
    #[non_exhaustive]
    NoDnsResolver,

    /// The URI did not specify a host
    #[non_exhaustive]
    MissingHost,

    /// The URI did not refer to the loopback interface
    #[non_exhaustive]
    NotLoopback,

    /// DNS lookup failed when attempting to resolve the host to an IP Address for validation.
    DnsLookupFailed(ResolveDnsError),
}

/// Invalid Full URI
///
/// When the full URI setting is used, the URI must either be HTTPS or point to a loopback interface.
#[derive(Debug)]
pub struct InvalidFullUriError {
    kind: InvalidFullUriErrorKind,
}

impl Display for InvalidFullUriError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use InvalidFullUriErrorKind::*;
        match self.kind {
            InvalidUri(_) => write!(f, "URI was invalid"),
            MissingHost => write!(f, "URI did not specify a host"),
            NotLoopback => {
                write!(f, "URI did not refer to the loopback interface")
            }
            DnsLookupFailed(_) => {
                write!(
                    f,
                    "failed to perform DNS lookup while validating URI"
                )
            }
            NoDnsResolver => write!(f, "no DNS resolver was provided. Enable `rt-tokio` or provide a `dns` resolver to the builder.")
        }
    }
}

impl Error for InvalidFullUriError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use InvalidFullUriErrorKind::*;
        match &self.kind {
            InvalidUri(err) => Some(err),
            DnsLookupFailed(err) => Some(err as _),
            _ => None,
        }
    }
}

impl From<InvalidFullUriErrorKind> for InvalidFullUriError {
    fn from(kind: InvalidFullUriErrorKind) -> Self {
        Self { kind }
    }
}

/// Validate that `uri` is valid to be used as a full provider URI
/// Either:
/// 1. The URL is uses `https`
/// 2. The URL refers to a loopback device. If a URL contains a domain name instead of an IP address,
/// a DNS lookup will be performed. ALL resolved IP addresses MUST refer to a loopback interface, or
/// the credentials provider will return `CredentialsError::InvalidConfiguration`
async fn validate_full_uri(
    uri: &str,
    dns: Option<SharedDnsResolver>,
) -> Result<Uri, InvalidFullUriError> {
    let uri = uri
        .parse::<Uri>()
        .map_err(InvalidFullUriErrorKind::InvalidUri)?;
    if uri.scheme() == Some(&Scheme::HTTPS) {
        return Ok(uri);
    }
    // For HTTP URIs, we need to validate that it points to a loopback address
    let host = uri.host().ok_or(InvalidFullUriErrorKind::MissingHost)?;
    let is_loopback = match host.parse::<IpAddr>() {
        Ok(addr) => addr.is_loopback(),
        Err(_domain_name) => {
            let dns = dns.ok_or(InvalidFullUriErrorKind::NoDnsResolver)?;
            dns.resolve_dns(host)
                .await
                .map_err(|err| InvalidFullUriErrorKind::DnsLookupFailed(ResolveDnsError::new(err)))?
                .iter()
                    .all(|addr| {
                        if !addr.is_loopback() {
                            tracing::warn!(
                                addr = ?addr,
                                "HTTP credential provider cannot be used: Address does not resolve to the loopback interface."
                            )
                        };
                        addr.is_loopback()
                    })
        }
    };
    match is_loopback {
        true => Ok(uri),
        false => Err(InvalidFullUriErrorKind::NotLoopback.into()),
    }
}

/// Default DNS resolver impl
///
/// DNS resolution is required to validate that provided URIs point to the loopback interface
#[cfg(any(not(feature = "rt-tokio"), target_family = "wasm"))]
fn default_dns() -> Option<SharedDnsResolver> {
    None
}
#[cfg(all(feature = "rt-tokio", not(target_family = "wasm")))]
fn default_dns() -> Option<SharedDnsResolver> {
    use aws_smithy_runtime::client::dns::TokioDnsResolver;
    Some(TokioDnsResolver::new().into_shared())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::provider_config::ProviderConfig;
    use crate::test_case::GenericTestResult;
    use aws_credential_types::provider::ProvideCredentials;
    use aws_credential_types::Credentials;
    use aws_smithy_async::future::never::Never;
    use aws_smithy_async::rt::sleep::TokioSleep;
    use aws_smithy_runtime::client::http::test_util::{ReplayEvent, StaticReplayClient};
    use aws_smithy_runtime_api::client::dns::DnsFuture;
    use aws_smithy_runtime_api::client::http::HttpClient;
    use aws_smithy_runtime_api::shared::IntoShared;
    use aws_smithy_types::body::SdkBody;
    use aws_types::os_shim_internal::Env;
    use futures_util::FutureExt;
    use http::header::AUTHORIZATION;
    use http::Uri;
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::error::Error;
    use std::net::IpAddr;
    use std::time::{Duration, UNIX_EPOCH};
    use tracing_test::traced_test;

    fn provider(env: Env, http_client: impl HttpClient + 'static) -> EcsCredentialsProvider {
        let provider_config = ProviderConfig::empty()
            .with_env(env)
            .with_http_client(http_client)
            .with_sleep_impl(TokioSleep::new());
        Builder::default().configure(&provider_config).build()
    }

    #[derive(Deserialize)]
    struct EcsUriTest {
        env: HashMap<String, String>,
        result: GenericTestResult<String>,
    }

    impl EcsUriTest {
        async fn check(&self) {
            let env = Env::from(self.env.clone());
            let uri = Provider::uri(env, Some(TestDns::default().into_shared()))
                .await
                .map(|uri| uri.to_string());
            self.result.assert_matches(uri);
        }
    }

    #[tokio::test]
    async fn run_config_tests() -> Result<(), Box<dyn Error>> {
        let test_cases = std::fs::read_to_string("test-data/ecs-tests.json")?;
        #[derive(Deserialize)]
        struct TestCases {
            tests: Vec<EcsUriTest>,
        }

        let test_cases: TestCases = serde_json::from_str(&test_cases)?;
        let test_cases = test_cases.tests;
        for test in test_cases {
            test.check().await
        }
        Ok(())
    }

    #[test]
    fn validate_uri_https() {
        // over HTTPs, any URI is fine
        let dns = Some(NeverDns.into_shared());
        assert_eq!(
            validate_full_uri("https://amazon.com", None)
                .now_or_never()
                .unwrap()
                .expect("valid"),
            Uri::from_static("https://amazon.com")
        );
        // over HTTP, it will try to lookup
        assert!(
            validate_full_uri("http://amazon.com", dns)
                .now_or_never()
                .is_none(),
            "DNS lookup should occur, but it will never return"
        );

        let no_dns_error = validate_full_uri("http://amazon.com", None)
            .now_or_never()
            .unwrap()
            .expect_err("DNS service is required");
        assert!(
            matches!(
                no_dns_error,
                InvalidFullUriError {
                    kind: InvalidFullUriErrorKind::NoDnsResolver
                }
            ),
            "expected no dns service, got: {}",
            no_dns_error
        );
    }

    #[test]
    fn valid_uri_loopback() {
        assert_eq!(
            validate_full_uri("http://127.0.0.1:8080/get-credentials", None)
                .now_or_never()
                .unwrap()
                .expect("valid uri"),
            Uri::from_static("http://127.0.0.1:8080/get-credentials")
        );

        let err = validate_full_uri("http://192.168.10.120/creds", None)
            .now_or_never()
            .unwrap()
            .expect_err("not a loopback");
        assert!(matches!(
            err,
            InvalidFullUriError {
                kind: InvalidFullUriErrorKind::NotLoopback
            }
        ));
    }

    #[test]
    fn all_addrs_local() {
        let dns = Some(
            TestDns::with_fallback(vec![
                "127.0.0.1".parse().unwrap(),
                "127.0.0.2".parse().unwrap(),
            ])
            .into_shared(),
        );
        let resp = validate_full_uri("http://localhost:8888", dns)
            .now_or_never()
            .unwrap();
        assert!(resp.is_ok(), "Should be valid: {:?}", resp);
    }

    #[test]
    fn all_addrs_not_local() {
        let dns = Some(
            TestDns::with_fallback(vec![
                "127.0.0.1".parse().unwrap(),
                "192.168.0.1".parse().unwrap(),
            ])
            .into_shared(),
        );
        let resp = validate_full_uri("http://localhost:8888", dns)
            .now_or_never()
            .unwrap();
        assert!(
            matches!(
                resp,
                Err(InvalidFullUriError {
                    kind: InvalidFullUriErrorKind::NotLoopback
                })
            ),
            "Should be invalid: {:?}",
            resp
        );
    }

    fn creds_request(uri: &str, auth: Option<&str>) -> http::Request<SdkBody> {
        let mut builder = http::Request::builder();
        if let Some(auth) = auth {
            builder = builder.header(AUTHORIZATION, auth);
        }
        builder.uri(uri).body(SdkBody::empty()).unwrap()
    }

    fn ok_creds_response() -> http::Response<SdkBody> {
        http::Response::builder()
            .status(200)
            .body(SdkBody::from(
                r#" {
                       "AccessKeyId" : "AKID",
                       "SecretAccessKey" : "SECRET",
                       "Token" : "TOKEN....=",
                       "Expiration" : "2009-02-13T23:31:30Z"
                     }"#,
            ))
            .unwrap()
    }

    #[track_caller]
    fn assert_correct(creds: Credentials) {
        assert_eq!(creds.access_key_id(), "AKID");
        assert_eq!(creds.secret_access_key(), "SECRET");
        assert_eq!(creds.session_token().unwrap(), "TOKEN....=");
        assert_eq!(
            creds.expiry().unwrap(),
            UNIX_EPOCH + Duration::from_secs(1234567890)
        );
    }

    #[tokio::test]
    async fn load_valid_creds_auth() {
        let env = Env::from_slice(&[
            ("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/credentials"),
            ("AWS_CONTAINER_AUTHORIZATION_TOKEN", "Basic password"),
        ]);
        let http_client = StaticReplayClient::new(vec![ReplayEvent::new(
            creds_request("http://169.254.170.2/credentials", Some("Basic password")),
            ok_creds_response(),
        )]);
        let provider = provider(env, http_client.clone());
        let creds = provider
            .provide_credentials()
            .await
            .expect("valid credentials");
        assert_correct(creds);
        http_client.assert_requests_match(&[]);
    }

    #[tokio::test]
    async fn retry_5xx() {
        let env = Env::from_slice(&[("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/credentials")]);
        let http_client = StaticReplayClient::new(vec![
            ReplayEvent::new(
                creds_request("http://169.254.170.2/credentials", None),
                http::Response::builder()
                    .status(500)
                    .body(SdkBody::empty())
                    .unwrap(),
            ),
            ReplayEvent::new(
                creds_request("http://169.254.170.2/credentials", None),
                ok_creds_response(),
            ),
        ]);
        tokio::time::pause();
        let provider = provider(env, http_client.clone());
        let creds = provider
            .provide_credentials()
            .await
            .expect("valid credentials");
        assert_correct(creds);
    }

    #[tokio::test]
    async fn load_valid_creds_no_auth() {
        let env = Env::from_slice(&[("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/credentials")]);
        let http_client = StaticReplayClient::new(vec![ReplayEvent::new(
            creds_request("http://169.254.170.2/credentials", None),
            ok_creds_response(),
        )]);
        let provider = provider(env, http_client.clone());
        let creds = provider
            .provide_credentials()
            .await
            .expect("valid credentials");
        assert_correct(creds);
        http_client.assert_requests_match(&[]);
    }

    // ignored by default because it relies on actual DNS resolution
    #[allow(unused_attributes)]
    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn real_dns_lookup() {
        let dns = Some(
            default_dns()
                .expect("feature must be enabled")
                .into_shared(),
        );
        let err = validate_full_uri("http://www.amazon.com/creds", dns.clone())
            .await
            .expect_err("not a loopback");
        assert!(
            matches!(
                err,
                InvalidFullUriError {
                    kind: InvalidFullUriErrorKind::NotLoopback
                }
            ),
            "{:?}",
            err
        );
        assert!(logs_contain(
            "Address does not resolve to the loopback interface"
        ));
        validate_full_uri("http://localhost:8888/creds", dns)
            .await
            .expect("localhost is the loopback interface");
    }

    /// Always returns the same IP addresses
    #[derive(Clone, Debug)]
    struct TestDns {
        addrs: HashMap<String, Vec<IpAddr>>,
        fallback: Vec<IpAddr>,
    }

    /// Default that returns a loopback for `localhost` and a non-loopback for all other hostnames
    impl Default for TestDns {
        fn default() -> Self {
            let mut addrs = HashMap::new();
            addrs.insert(
                "localhost".into(),
                vec!["127.0.0.1".parse().unwrap(), "127.0.0.2".parse().unwrap()],
            );
            TestDns {
                addrs,
                // non-loopback address
                fallback: vec!["72.21.210.29".parse().unwrap()],
            }
        }
    }

    impl TestDns {
        fn with_fallback(fallback: Vec<IpAddr>) -> Self {
            TestDns {
                addrs: Default::default(),
                fallback,
            }
        }
    }

    impl ResolveDns for TestDns {
        fn resolve_dns<'a>(&'a self, name: &'a str) -> DnsFuture<'a> {
            DnsFuture::ready(Ok(self.addrs.get(name).unwrap_or(&self.fallback).clone()))
        }
    }

    #[derive(Debug)]
    struct NeverDns;
    impl ResolveDns for NeverDns {
        fn resolve_dns<'a>(&'a self, _name: &'a str) -> DnsFuture<'a> {
            DnsFuture::new(async {
                Never::new().await;
                unreachable!()
            })
        }
    }
}
