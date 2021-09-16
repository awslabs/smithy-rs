/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Raw IMDSv2 Client
//!
//! Client for direct access to IMDSv2.

use std::borrow::Cow;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::time::Duration;

use aws_http::user_agent::{ApiMetadata, AwsUserAgent, UserAgentStage};
use aws_types::os_shim_internal::{Env, Fs};
use bytes::Bytes;
use http::uri::InvalidUri;
use http::{Response, Uri};
use smithy_client::{erase::DynConnector, SdkSuccess};
use smithy_client::{retry, SdkError};
use smithy_http::body::SdkBody;
use smithy_http::endpoint::Endpoint;
use smithy_http::operation;
use smithy_http::operation::{Metadata, Operation};
use smithy_http::response::ParseStrictResponse;
use smithy_http::retry::ClassifyResponse;
use smithy_http_tower::map_request::{
    AsyncMapRequestLayer, AsyncMapRequestService, MapRequestLayer, MapRequestService,
};
use smithy_types::retry::{ErrorKind, RetryKind};

use crate::connector::expect_connector;
use crate::imds::client::token::TokenMiddleware;
use crate::profile::ProfileParseError;
use crate::provider_config::ProviderConfig;
use crate::{profile, PKG_VERSION};

const USER_AGENT: AwsUserAgent =
    AwsUserAgent::new_from_environment(ApiMetadata::new("imds", PKG_VERSION));

mod token;

// 6 hours
const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(21_600);
const DEFAULT_RETRIES: u32 = 3;

/// IMDSv2 Client
///
/// Client for IMDSv2. This client handles fetching tokens, retrying on failure, and token
/// caching according to the specified token TTL.
///
/// **NOTE:** This client ONLY supports IMDSv2. It will not fallback to IMDSv1. See
/// [transitioning to IMDSv2](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html#instance-metadata-transition-to-version-2)
/// for more information.
///
/// # Client Configuration
/// The IMDS client can load configuration explicitly, via environment variables, or via
/// `~/.aws/config`. It will first attempt to resolve an endpoint override. If no endpoint
/// override exists, it will attempt to resolve an [`EndpointMode`]. If no
/// [`EndpointMode`] override exists, it will fallback to [`IpV4`](EndpointMode::IpV4). An exhaustive
/// list is below:
///
/// ## Endpoint configuration list
/// 1. Explicit configuration of `Endpoint` via the [builder](Builder):
/// ```rust
/// use aws_config::imds::client::Client;
/// use http::Uri;
/// # async fn docs() {
/// let client = Client::builder()
///   .endpoint(Uri::from_static("http://customidms:456/"))
///   .build()
///   .await;
/// # }
/// ```
///
/// 2. The `AWS_EC2_METADATA_SERVICE_ENDPOINT` environment variable. Note: If this environment variable
/// is set, it MUST contain to a valid URI or client construction will fail.
///
/// 3. The `ec2_metadata_service_endpoint` field in `~/.aws/config`:
/// ```ini
/// [default]
/// # ... other configuration
/// ec2_metadata_service_endpoint = http://my-custom-endpoint:444
/// ```
///
/// 4. An explicitly set endpoint mode:
/// ```rust
/// use aws_config::imds::client::{Client, EndpointMode};
/// # async fn docs() {
/// let client = Client::builder().endpoint_mode(EndpointMode::IpV6).build().await;
/// # }
/// ```
///
/// 5. An [endpoint mode](EndpointMode) loaded from the `AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE` environment
/// variable. Valid values: `IPv4`, `IPv6`
///
/// 6. An [endpoint mode](EndpointMode) loaded from the `ec2_metadata_service_endpoint_mode` field in
/// `~/.aws/config`:
/// ```ini
/// [default]
/// # ... other configuration
/// ec2_metadata_service_endpoint_mode = IPv4
/// ```
///
/// 7. The default value of `http://169.254.169.254` will be used.
///
///
pub struct Client {
    endpoint: Endpoint,
    inner: smithy_client::Client<DynConnector, ImdsMiddleware>,
}

impl Client {
    /// IMDS client builder
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Retrieve information from IMDS
    ///
    /// This method will handle loading and caching a session token, combining the `path` with the
    /// configured IMDS endpoint, and retrying potential errors.
    ///
    /// For more information about IMDSv2 methods and functionality, see
    /// [Instance metadata and user data](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-instance-metadata.html)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use aws_config::imds::client::Client;
    /// # async fn docs() {
    /// let client = Client::builder().build().await.expect("valid client");
    /// let ami_id = client
    ///   .get("/latest/meta-data/ami-id")
    ///   .await
    ///   .expect("failure communicating with IMDS");
    /// # }
    /// ```
    pub async fn get(&self, path: &str) -> Result<String, SdkError<ImdsError>> {
        let operation = self
            .make_operation(path)
            .map_err(|err| SdkError::ConstructionFailure(err.into()))?;
        self.inner.call(operation).await
    }

    /// Creates a smithy_http Operation to for `path`
    /// - Convert the path to a URI
    /// - Set the base endpoint on the URI
    /// - Add a user agent
    fn make_operation(
        &self,
        path: &str,
    ) -> Result<Operation<ImdsGetResponseHandler, ImdsErrorPolicy>, ImdsError> {
        let mut base_uri: Uri = path.parse().map_err(|_| ImdsError::InvalidPath)?;
        self.endpoint.set_endpoint(&mut base_uri, None);
        let request = http::Request::builder()
            .uri(base_uri)
            .body(SdkBody::empty())
            .expect("valid request");
        let mut request = operation::Request::new(request);
        request.properties_mut().insert(USER_AGENT);
        Ok(Operation::new(request, ImdsGetResponseHandler)
            .with_metadata(Metadata::new("get", "imds"))
            .with_retry_policy(ImdsErrorPolicy))
    }
}

/// An error retrieving metdata from IMDS
#[derive(Debug)]
#[non_exhaustive]
pub enum ImdsError {
    /// An IMDSv2 Token could not be loaded
    ///
    /// Requests to IMDS must be accompanied by a token obtained via a `PUT` request. This is handled
    /// transparently by the [`Client`].
    FailedToLoadToken(SdkError<TokenError>),

    /// The `path` was invalid for an IMDS request
    ///
    /// The `path` parameter must be a valid URI path segment, and it must begin with `/`.
    InvalidPath,

    /// The response returned from IMDS was not valid UTF-8.
    ///
    /// This should never occur during normal operation.
    Utf8Error,

    /// An error response was returned from IMDS
    ErrorResponse {
        /// The returned status code
        code: u16,
    },
}

impl Display for ImdsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ImdsError::FailedToLoadToken(inner) => {
                write!(f, "failed to load session token: {}", inner)
            }
            ImdsError::InvalidPath => write!(
                f,
                "IMDS path was not a valid URI. Hint: Does it begin with `/`?"
            ),
            ImdsError::Utf8Error => write!(f, "Response from IMDS was not valid UTF-8"),
            ImdsError::ErrorResponse { code } => write!(f, "Error response from IMDS (code: {}). Consult the `raw` field of the parent error for more information.", code),
        }
    }
}

impl Error for ImdsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self {
            ImdsError::FailedToLoadToken(inner) => Some(inner),
            _ => None,
        }
    }
}

/// IMDS Middleware
///
/// The IMDS middleware includes a token-loader & a UserAgent stage
#[derive(Clone)]
struct ImdsMiddleware {
    token_loader: TokenMiddleware,
}

impl<S> tower::Layer<S> for ImdsMiddleware {
    type Service = AsyncMapRequestService<MapRequestService<S, UserAgentStage>, TokenMiddleware>;

    fn layer(&self, inner: S) -> Self::Service {
        AsyncMapRequestLayer::for_mapper(self.token_loader.clone())
            .layer(MapRequestLayer::for_mapper(UserAgentStage::new()).layer(inner))
    }
}

#[derive(Copy, Clone)]
struct ImdsGetResponseHandler;

impl ParseStrictResponse for ImdsGetResponseHandler {
    type Output = Result<String, ImdsError>;

    fn parse(&self, response: &Response<Bytes>) -> Self::Output {
        if response.status().is_success() {
            std::str::from_utf8(response.body().as_ref())
                .map(|data| data.to_string())
                .map_err(|_| ImdsError::Utf8Error)
        } else {
            Err(ImdsError::ErrorResponse {
                code: response.status().as_u16(),
            })
        }
    }
}

/// IMDSv2 Endpoint Mode
///
/// IMDS can be accessed in two ways:
/// 1. Via the IpV4 endpoint: `http://169.254.169.254`
/// 2. Via the Ipv6 endpoint: `http://[fd00:ec2::254]`
#[derive(Debug)]
#[non_exhaustive]
pub enum EndpointMode {
    /// IpV4 mode: `http://169.254.169.254`
    ///
    /// This mode is the default unless otherwise specified.
    IpV4,
    /// IpV6 mode: `http://[fd00:ec2::254]`
    IpV6,
}

/// Invalid Endpoint Mode
#[derive(Debug, Clone)]
pub struct InvalidEndpointMode(String);

impl Display for InvalidEndpointMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` is not a valid endpoint mode. Valid values are [`IPv4`, `IPv6`]",
            &self.0
        )
    }
}

impl Error for InvalidEndpointMode {}

impl FromStr for EndpointMode {
    type Err = InvalidEndpointMode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "IPv4" => Ok(EndpointMode::IpV4),
            "IPv6" => Ok(EndpointMode::IpV6),
            other => Err(InvalidEndpointMode(other.to_owned())),
        }
    }
}

impl EndpointMode {
    /// IMDS URI for this endpoint mode
    fn endpoint(&self) -> Uri {
        match self {
            EndpointMode::IpV4 => Uri::from_static("http://169.254.169.254"),
            EndpointMode::IpV6 => Uri::from_static("http://[fd00:ec2::254]"),
        }
    }
}

/// IMDSv2 Client Builder
#[derive(Default, Debug)]
pub struct Builder {
    num_retries: Option<u32>,
    endpoint: Option<EndpointSource>,
    mode_override: Option<EndpointMode>,
    token_ttl: Option<Duration>,
    config: Option<ProviderConfig>,
}

/// Error constructing IMDSv2 Client
#[derive(Debug)]
pub enum BuildError {
    /// The endpoint mode was invalid
    InvalidEndpointMode(InvalidEndpointMode),

    /// The AWS Profile (eg. `~/.aws/config`) was invalid
    InvalidProfile(ProfileParseError),

    /// The specified endpoint was not a valid URI
    InvalidEndpointUri(InvalidUri),
}

impl Display for BuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to build IMDS client: ")?;
        match self {
            BuildError::InvalidEndpointMode(e) => write!(f, "{}", e),
            BuildError::InvalidProfile(e) => write!(f, "{}", e),
            BuildError::InvalidEndpointUri(e) => write!(f, "{}", e),
        }
    }
}

impl Error for BuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            BuildError::InvalidEndpointMode(e) => Some(e),
            BuildError::InvalidProfile(e) => Some(e),
            BuildError::InvalidEndpointUri(e) => Some(e),
        }
    }
}

impl Builder {
    /// Override the number of retries for fetching tokens & metdata
    ///
    /// By default, 3 retries will be made.
    pub fn retries(mut self, retries: u32) -> Self {
        self.num_retries = Some(retries);
        self
    }

    /// Configure generic options of the [`Client`]
    ///
    /// # Examples
    /// ```rust
    /// # use aws_config::imds::Client;
    ///  async fn test() {
    /// use aws_config::provider_config::ProviderConfig;
    /// let provider = Client::builder()
    ///     .configure(&ProviderConfig::with_default_region().await)
    ///     .build();
    /// # }
    /// ```
    pub fn configure(mut self, provider_config: &ProviderConfig) -> Self {
        self.config = Some(provider_config.clone());
        self
    }

    /// Override the endpoint for the [`Client`]
    ///
    /// By default, the client will resolve an endpoint from the environment, AWS config, and endpoint mode.
    ///
    /// See [`Client`] for more information.
    pub fn endpoint(mut self, endpoint: impl Into<Uri>) -> Self {
        self.endpoint = Some(EndpointSource::Explicit(endpoint.into()));
        self
    }

    /// Override the endpoint mode for [`Client`]
    ///
    /// * When set to [`IpV4`](EndpointMode::IpV4), the endpoint will be `http://169.254.169.254`.
    /// * When set to [`IpV6`](EndpointMode::IpV6), the endpoint will be `http://[fd00:ec2::254]`.
    pub fn endpoint_mode(mut self, mode: EndpointMode) -> Self {
        self.mode_override = Some(mode);
        self
    }

    /// Override the time-to-live for the session token
    ///
    /// Requests to IMDS utilize a session token for authentication. By default, session tokens last
    /// for 6 hours. When the TTL for the token expires, a new token must be retrieved from the
    /// metadata service.
    pub fn token_ttl(mut self, ttl: Duration) -> Self {
        self.token_ttl = Some(ttl);
        self
    }

    /* TODO: Support customizing the port explicitly */
    /*
    pub fn port(mut self, port: u32) -> Self {
        self.port_override = Some(port);
        self
    }*/

    /// Build an IMDSv2 Client
    pub async fn build(self) -> Result<Client, BuildError> {
        let config = self.config.unwrap_or_default();
        let connector = expect_connector(config.connector().cloned());
        let endpoint_source = self
            .endpoint
            .unwrap_or_else(|| EndpointSource::Env(config.env(), config.fs()));
        let endpoint = endpoint_source.endpoint(self.mode_override).await?;
        let endpoint = Endpoint::immutable(endpoint);
        let retry_config =
            retry::Config::default().with_max_retries(self.num_retries.unwrap_or(DEFAULT_RETRIES));
        let token_loader = token::TokenMiddleware::new(
            connector.clone(),
            config.time_source(),
            endpoint.clone(),
            self.token_ttl.unwrap_or(DEFAULT_TOKEN_TTL),
            retry_config.clone(),
        );
        let middleware = ImdsMiddleware { token_loader };
        let inner_client = smithy_client::Builder::new()
            .connector(connector.clone())
            .middleware(middleware)
            .build()
            .with_retry_config(retry_config);
        let client = Client {
            endpoint,
            inner: inner_client,
        };
        Ok(client)
    }
}

mod env {
    pub const ENDPOINT: &str = "AWS_EC2_METADATA_SERVICE_ENDPOINT";
    pub const ENDPOINT_MODE: &str = "AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE";
}

mod profile_keys {
    pub const ENDPOINT: &str = "ec2_metadata_service_endpoint";
    pub const ENDPOINT_MODE: &str = "ec2_metadata_service_endpoint_mode";
}

/// Endpoint Configuration Abstraction
#[derive(Debug)]
enum EndpointSource {
    Explicit(Uri),
    Env(Env, Fs),
}

impl EndpointSource {
    async fn endpoint(&self, mode_override: Option<EndpointMode>) -> Result<Uri, BuildError> {
        match self {
            EndpointSource::Explicit(uri) => {
                if mode_override.is_some() {
                    tracing::warn!(endpoint = ?uri, mode = ?mode_override,
                        "Endpoint mode override was set in combination with an explicit endpoint. \
                        The mode override will be ignored.")
                }
                Ok(uri.clone())
            }
            EndpointSource::Env(env, fs) => {
                // load an endpoint override from the environment
                let profile = profile::load(fs, env)
                    .await
                    .map_err(BuildError::InvalidProfile)?;
                let uri_override = if let Ok(uri) = env.get(env::ENDPOINT) {
                    Some(Cow::Owned(uri))
                } else {
                    profile.get(profile_keys::ENDPOINT).map(Cow::Borrowed)
                };
                if let Some(uri) = uri_override {
                    return Uri::try_from(uri.as_ref()).map_err(BuildError::InvalidEndpointUri);
                }

                // if not, load a endpoint mode from the environment
                let mode = if let Some(mode) = mode_override {
                    mode
                } else if let Ok(mode) = env.get(env::ENDPOINT_MODE) {
                    mode.parse::<EndpointMode>()
                        .map_err(BuildError::InvalidEndpointMode)?
                } else if let Some(mode) = profile.get(profile_keys::ENDPOINT_MODE) {
                    mode.parse::<EndpointMode>()
                        .map_err(BuildError::InvalidEndpointMode)?
                } else {
                    EndpointMode::IpV4
                };

                Ok(mode.endpoint())
            }
        }
    }
}

/// Error retrieving token from IMDS
#[derive(Debug)]
pub enum TokenError {
    /// The token was invalid
    ///
    /// Because tokens must be eventually sent as a header, the token must be a valid header value.
    InvalidToken,

    /// No TTL was sent
    ///
    /// The token response must include a time-to-live indicating the lifespan of the token.
    NoTtl,

    /// The TTL was invalid
    ///
    /// The TTL must be a valid positive integer.
    InvalidTtl,

    /// Invalid Parameters
    ///
    /// The request to load a token was malformed. This indicates an SDK bug.
    InvalidParameters,

    /// Forbidden
    ///
    /// IMDS is disabled or has been disallowed via permissions.
    Forbidden,
}

impl Display for TokenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::InvalidToken => write!(f, "Invalid Token"),
            TokenError::NoTtl => write!(f, "Token response did not contain a TTL header"),
            TokenError::InvalidTtl => write!(f, "The returned TTL was invalid"),
            TokenError::InvalidParameters => {
                write!(f, "Invalid request parameters. This indicates an SDK bug.")
            }
            TokenError::Forbidden => write!(
                f,
                "Request forbidden: IMDS is disabled or the caller has insufficient permissions."
            ),
        }
    }
}

impl Error for TokenError {}

#[derive(Clone)]
struct ImdsErrorPolicy;
impl ImdsErrorPolicy {
    fn classify(response: &operation::Response) -> RetryKind {
        let status = response.http().status();
        match status {
            _ if status.is_server_error() => RetryKind::Error(ErrorKind::ServerError),
            // 401 indicates that the token has expired, this is retryable
            _ if status.as_u16() == 401 => RetryKind::Error(ErrorKind::ServerError),
            _ => RetryKind::NotRetryable,
        }
    }
}

/// IMDS Retry Policy
///
/// Possible status codes:
/// - 200 (OK)
/// - 400 (Missing or invalid parameters) **Not Retryable**
/// - 401 (Unauthorized, expired token) **Retryable**
/// - 403 (IMDS disabled): **Not Retryable**
/// - 404 (Not found): **Not Retryable**
/// - >=500 (server error): **Retryable**
impl<T, E> ClassifyResponse<SdkSuccess<T>, SdkError<E>> for ImdsErrorPolicy {
    fn classify(&self, response: Result<&SdkSuccess<T>, &SdkError<E>>) -> RetryKind {
        match response {
            Ok(_) => RetryKind::NotRetryable,
            Err(SdkError::ResponseError { raw, .. }) | Err(SdkError::ServiceError { raw, .. }) => {
                ImdsErrorPolicy::classify(raw)
            }
            _ => RetryKind::NotRetryable,
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::error::Error;
    use std::time::{Duration, UNIX_EPOCH};

    use aws_hyper::DynConnector;
    use aws_types::os_shim_internal::{Env, Fs, ManualTimeSource, TimeSource};
    use http::Uri;
    use serde::Deserialize;
    use smithy_client::test_connection::{capture_request, TestConnection};
    use smithy_http::body::SdkBody;
    use tracing_test::traced_test;

    use crate::imds::client::{Client, EndpointMode};
    use crate::provider_config::ProviderConfig;
    use http::header::USER_AGENT;

    const TOKEN_A: &str = "AQAEAFTNrA4eEGx0AQgJ1arIq_Cc-t4tWt3fB0Hd8RKhXlKc5ccvhg==";
    const TOKEN_B: &str = "alternatetoken==";

    fn token_request(base: &str, ttl: u32) -> http::Request<SdkBody> {
        http::Request::builder()
            .uri(format!("{}/latest/api/token", base))
            .header("x-aws-ec2-metadata-token-ttl-seconds", ttl)
            .method("PUT")
            .body(SdkBody::empty())
            .unwrap()
    }

    fn token_response(ttl: u32, token: &'static str) -> http::Response<&'static str> {
        http::Response::builder()
            .status(200)
            .header("X-aws-ec2-metadata-token-ttl-seconds", ttl)
            .body(token)
            .unwrap()
    }

    fn imds_request(path: &'static str, token: &str) -> http::Request<SdkBody> {
        http::Request::builder()
            .uri(Uri::from_static(path))
            .method("GET")
            .header("x-aws-ec2-metadata-token", token)
            .body(SdkBody::empty())
            .unwrap()
    }

    fn imds_response(body: &'static str) -> http::Response<&'static str> {
        http::Response::builder().status(200).body(body).unwrap()
    }

    async fn make_client<T>(conn: &TestConnection<T>) -> super::Client
    where
        SdkBody: From<T>,
        T: Send + 'static,
    {
        super::Client::builder()
            .configure(
                &ProviderConfig::no_configuration().with_connector(DynConnector::new(conn.clone())),
            )
            .build()
            .await
            .expect("valid client")
    }

    #[tokio::test]
    async fn client_caches_token() {
        let connection = TestConnection::new(vec![
            (
                token_request("http://169.254.169.254", 21600),
                token_response(21600, TOKEN_A),
            ),
            (
                imds_request("http://169.254.169.254/latest/metadata", TOKEN_A),
                imds_response(r#"test-imds-output"#),
            ),
            (
                imds_request("http://169.254.169.254/latest/metadata2", TOKEN_A),
                imds_response("output2"),
            ),
        ]);
        let client = make_client(&connection).await;
        // load once
        let metadata = client.get("/latest/metadata").await.expect("failed");
        assert_eq!(metadata, "test-imds-output");
        // load again: the cached token should be used
        let metadata = client.get("/latest/metadata2").await.expect("failed");
        assert_eq!(metadata, "output2");
        connection.assert_requests_match(&[]);
    }

    #[tokio::test]
    async fn token_can_expire() {
        let connection = TestConnection::new(vec![
            (
                token_request("http://[fd00:ec2::254]", 600),
                token_response(600, TOKEN_A),
            ),
            (
                imds_request("http://[fd00:ec2::254]/latest/metadata", TOKEN_A),
                imds_response(r#"test-imds-output1"#),
            ),
            (
                token_request("http://[fd00:ec2::254]", 600),
                token_response(600, TOKEN_B),
            ),
            (
                imds_request("http://[fd00:ec2::254]/latest/metadata", TOKEN_B),
                imds_response(r#"test-imds-output2"#),
            ),
        ]);
        let mut time_source = ManualTimeSource::new(UNIX_EPOCH);
        let client = super::Client::builder()
            .configure(
                &ProviderConfig::no_configuration()
                    .with_connector(DynConnector::new(connection.clone()))
                    .with_time_source(TimeSource::manual(&time_source)),
            )
            .endpoint_mode(EndpointMode::IpV6)
            .token_ttl(Duration::from_secs(600))
            .build()
            .await
            .expect("valid client");

        let resp1 = client.get("/latest/metadata").await.expect("success");
        // now the cached credential has expired
        time_source.advance(Duration::from_secs(600));
        let resp2 = client.get("/latest/metadata").await.expect("success");
        connection.assert_requests_match(&[]);
        assert_eq!(resp1, "test-imds-output1");
        assert_eq!(resp2, "test-imds-output2");
    }

    /// 500 error during the GET should be retried
    #[tokio::test]
    #[traced_test]
    async fn retry_500() {
        let connection = TestConnection::new(vec![
            (
                token_request("http://169.254.169.254", 21600),
                token_response(21600, TOKEN_A),
            ),
            (
                imds_request("http://169.254.169.254/latest/metadata", TOKEN_A),
                http::Response::builder().status(500).body("").unwrap(),
            ),
            (
                imds_request("http://169.254.169.254/latest/metadata", TOKEN_A),
                imds_response("ok"),
            ),
        ]);
        let client = make_client(&connection).await;
        assert_eq!(client.get("/latest/metadata").await.expect("success"), "ok");
        connection.assert_requests_match(&[]);

        // all requests should have a user agent header
        for request in connection.requests().iter() {
            assert!(request.actual.headers().get(USER_AGENT).is_some());
        }
    }

    /// 500 error during token acquisition should be retried
    #[tokio::test]
    #[traced_test]
    async fn retry_token_failure() {
        let connection = TestConnection::new(vec![
            (
                token_request("http://169.254.169.254", 21600),
                http::Response::builder().status(500).body("").unwrap(),
            ),
            (
                token_request("http://169.254.169.254", 21600),
                token_response(21600, TOKEN_A),
            ),
            (
                imds_request("http://169.254.169.254/latest/metadata", TOKEN_A),
                imds_response("ok"),
            ),
        ]);
        let client = make_client(&connection).await;
        assert_eq!(client.get("/latest/metadata").await.expect("success"), "ok");
        connection.assert_requests_match(&[]);
    }

    /// 403 responses from IMDS during token acquisition MUST NOT be retried
    #[tokio::test]
    #[traced_test]
    async fn no_403_retry() {
        let connection = TestConnection::new(vec![(
            token_request("http://169.254.169.254", 21600),
            http::Response::builder().status(403).body("").unwrap(),
        )]);
        let client = make_client(&connection).await;
        let err = client.get("/latest/metadata").await.expect_err("no token");
        assert!(format!("{}", err).contains("forbidden"), "{}", err);
        connection.assert_requests_match(&[]);
    }

    // since tokens are sent as headers, the tokens need to be valid header values
    #[tokio::test]
    async fn invalid_token() {
        let connection = TestConnection::new(vec![(
            token_request("http://169.254.169.254", 21600),
            token_response(21600, "replaced").map(|_| vec![1, 0]),
        )]);
        let client = make_client(&connection).await;
        let err = client.get("/latest/metadata").await.expect_err("no token");
        assert!(format!("{}", err).contains("Invalid Token"), "{}", err);
        connection.assert_requests_match(&[]);
    }

    #[tokio::test]
    async fn non_utf8_response() {
        let connection = TestConnection::new(vec![
            (
                token_request("http://169.254.169.254", 21600),
                token_response(21600, TOKEN_A).map(SdkBody::from),
            ),
            (
                imds_request("http://169.254.169.254/latest/metadata", TOKEN_A),
                http::Response::builder()
                    .status(200)
                    .body(SdkBody::from(vec![0xA0 as u8, 0xA1 as u8]))
                    .unwrap(),
            ),
        ]);
        let client = make_client(&connection).await;
        let err = client.get("/latest/metadata").await.expect_err("no token");
        assert!(format!("{}", err).contains("not valid UTF-8"), "{}", err);
        connection.assert_requests_match(&[]);
    }

    #[derive(Debug, Deserialize)]
    struct ImdsConfigTest {
        env: HashMap<String, String>,
        fs: HashMap<String, String>,
        endpoint_override: Option<String>,
        mode_override: Option<String>,
        result: Result<String, String>,
        docs: String,
    }

    #[tokio::test]
    async fn config_tests() -> Result<(), Box<dyn Error>> {
        let test_cases = std::fs::read_to_string("test-data/imds-config/imds-tests.json")?;
        #[derive(Deserialize)]
        struct TestCases {
            tests: Vec<ImdsConfigTest>,
        }

        let test_cases: TestCases = serde_json::from_str(&test_cases)?;
        let test_cases = test_cases.tests;
        for test in test_cases {
            check(test).await;
        }
        Ok(())
    }

    async fn check(test_case: ImdsConfigTest) {
        let (server, watcher) = capture_request(None);
        let provider_config = ProviderConfig::no_configuration()
            .with_env(Env::from(test_case.env))
            .with_fs(Fs::from_map(test_case.fs))
            .with_connector(DynConnector::new(server));
        let mut imds_client = Client::builder().configure(&provider_config);
        if let Some(endpoint_override) = test_case.endpoint_override {
            imds_client = imds_client.endpoint(endpoint_override.parse::<Uri>().unwrap());
        }

        if let Some(mode_override) = test_case.mode_override {
            imds_client = imds_client.endpoint_mode(mode_override.parse().unwrap());
        }

        let imds_client = imds_client.build().await;
        let (uri, imds_client) = match (&test_case.result, imds_client) {
            (Ok(uri), Ok(client)) => (uri, client),
            (Err(test), Ok(_client)) => panic!(
                "test should fail: {} but a valid client was made. {}",
                test, test_case.docs
            ),
            (Err(substr), Err(err)) => {
                assert!(
                    format!("{}", err).contains(substr),
                    "`{}` did not contain `{}`",
                    err,
                    substr
                );
                return;
            }
            (Ok(_uri), Err(e)) => panic!(
                "a valid client should be made but: {}. {}",
                e, test_case.docs
            ),
        };
        // this request will fail, we just want to capture the endpoint configuration
        let _ = imds_client.get("/hello").await;
        assert_eq!(&watcher.expect_request().uri().to_string(), uri);
    }
}
