/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Code related to creating signed URLs for logging in to RDS.
//!
//! For more information, see <https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/UsingWithRDS.IAMDBAuth.Connecting.html>

use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::http_request;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::identity::Identity;
use aws_types::region::Region;
use std::fmt;
use std::fmt::Debug;
use std::time::Duration;

const ACTION: &str = "connect";
const SERVICE: &str = "rds-db";

/// A signer that generates an auth token for a database.
///
/// ## Example
///
/// ```ignore
/// use crate::auth_token::{AuthTokenGenerator, Config};
///
/// #[tokio::main]
/// async fn main() {
///    let cfg = aws_config::load_defaults(BehaviorVersion::latest()).await;
///    let generator = AuthTokenGenerator::new(
///        Config::builder()
///            .hostname("zhessler-test-db.cp7a4mblr2ig.us-east-1.rds.amazonaws.com")
///            .port(5432)
///            .username("zhessler")
///            .build()
///            .expect("cfg is valid"),
///    );
///    let token = generator.auth_token(&cfg).await.unwrap();
///    println!("{token}");
/// }
/// ```
#[derive(Debug)]
pub struct AuthTokenGenerator {
    config: Config,
}

/// An auth token usable as a password for an RDS database.
///
/// This struct can be converted into a `&str` using the `Deref` trait or by calling `to_string()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthToken {
    inner: String,
}

impl AuthToken {
    /// Return the auth token as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl fmt::Display for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl AuthTokenGenerator {
    /// Given a `Config`, create a new RDS database login URL signer.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Return a signed URL usable as an auth token.
    pub async fn auth_token(
        &self,
        config: &aws_types::sdk_config::SdkConfig,
    ) -> Result<AuthToken, BoxError> {
        let credentials = self
            .config
            .credentials()
            .or(config.credentials_provider())
            .ok_or("credentials are required to create a signed URL for RDS")?
            .provide_credentials()
            .await?;
        let identity: Identity = credentials.into();
        let region = self
            .config
            .region()
            .or(config.region())
            .cloned()
            .unwrap_or_else(|| Region::new("us-east-1"));
        let time = config.time_source().ok_or("a time source is required")?;

        let mut signing_settings = SigningSettings::default();
        signing_settings.expires_in = Some(Duration::from_secs(
            self.config.expires_in().unwrap_or(900).min(900),
        ));
        signing_settings.signature_location = http_request::SignatureLocation::QueryParams;

        let signing_params = v4::SigningParams::builder()
            .identity(&identity)
            .region(region.as_ref())
            .name(SERVICE)
            .time(time.now())
            .settings(signing_settings)
            .build()?;

        let url = format!(
            "https://{}:{}/?Action={}&DBUser={}",
            self.config.hostname(),
            self.config.port(),
            ACTION,
            self.config.username()
        );
        let signable_request =
            SignableRequest::new("GET", &url, std::iter::empty(), SignableBody::empty())
                .expect("signable request");

        let (signing_instructions, _signature) =
            http_request::sign(signable_request, &signing_params.into())?.into_parts();

        let mut url = url::Url::parse(&url).unwrap();
        for (name, value) in signing_instructions.params() {
            url.query_pairs_mut().append_pair(name, value);
        }
        let inner = url.to_string().split_off("https://".len());

        Ok(AuthToken { inner })
    }
}

/// Configuration for an RDS auth URL signer.
#[derive(Debug, Clone)]
pub struct Config {
    /// The AWS credentials to sign requests with.
    ///
    /// Uses the default credential provider chain if not specified.
    credentials: Option<SharedCredentialsProvider>,

    /// The hostname of the database to connect to.
    hostname: String,

    /// The port number the database is listening on.
    port: u64,

    /// The region the database is located in. Uses the region inferred from the runtime if omitted.
    region: Option<Region>,

    /// The username to login as.
    username: String,

    /// The number of seconds the signed URL should be valid for.
    ///
    /// Maxes at 900 seconds.
    expires_in: Option<u64>,
}

impl Config {
    /// Create a new `SignerConfigBuilder`.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// The AWS credentials to sign requests with.
    pub fn credentials(&self) -> Option<SharedCredentialsProvider> {
        self.credentials.clone()
    }

    /// The hostname of the database to connect to.
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// The port number the database is listening on.
    pub fn port(&self) -> u64 {
        self.port
    }

    /// The region to sign requests with.
    pub fn region(&self) -> Option<&Region> {
        self.region.as_ref()
    }

    /// The DB username to login as.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// The number of seconds the signed URL should be valid for.
    ///
    /// Maxes out at 900 seconds.
    pub fn expires_in(&self) -> Option<u64> {
        self.expires_in
    }
}

/// A builder for [`Config`]s.
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    /// The AWS credentials to create the auth token with.
    ///
    /// Uses the default credential provider chain if not specified.
    credentials: Option<SharedCredentialsProvider>,

    /// The hostname of the database to connect to.
    hostname: Option<String>,

    /// The port number the database is listening on.
    port: Option<u64>,

    /// The region the database is located in. Uses the region inferred from the runtime if omitted.
    region: Option<Region>,

    /// The database username to login as.
    username: Option<String>,

    /// The number of seconds the auth token should be valid for.
    expires_in: Option<u64>,
}

impl ConfigBuilder {
    /// The AWS credentials to create the auth token with.
    ///
    /// Uses the default credential provider chain if not specified.
    pub fn credentials(mut self, credentials: impl ProvideCredentials + 'static) -> Self {
        self.credentials = Some(SharedCredentialsProvider::new(credentials));
        self
    }

    /// The hostname of the database to connect to.
    pub fn hostname(mut self, hostname: impl Into<String>) -> Self {
        self.hostname = Some(hostname.into());
        self
    }

    /// The port number the database is listening on.
    pub fn port(mut self, port: u64) -> Self {
        self.port = Some(port);
        self
    }

    /// The region the database is located in. Uses the region inferred from the runtime if omitted.
    pub fn region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// The database username to login as.
    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// The number of seconds the signed URL should be valid for.
    ///
    /// Maxes out at 900 seconds.
    pub fn expires_in(mut self, expires_in: u64) -> Self {
        self.expires_in = Some(expires_in);
        self
    }

    /// Consume this builder, returning an error if required fields are missing.
    /// Otherwise, return a new `SignerConfig`.
    pub fn build(self) -> Result<Config, BoxError> {
        Ok(Config {
            credentials: self.credentials,
            hostname: self.hostname.ok_or("A hostname is required")?,
            port: self.port.ok_or("a port is required")?,
            region: self.region,
            username: self.username.ok_or("a username is required")?,
            expires_in: self.expires_in,
        })
    }
}

#[cfg(test)]
mod test {
    use super::{AuthTokenGenerator, Config};
    use aws_credential_types::provider::SharedCredentialsProvider;
    use aws_credential_types::Credentials;
    use aws_smithy_async::test_util::ManualTimeSource;
    use aws_types::region::Region;
    use aws_types::SdkConfig;
    use std::time::{Duration, UNIX_EPOCH};

    #[tokio::test]
    async fn signing_works() {
        let time_source = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(1724709600));
        let sdk_config = SdkConfig::builder()
            .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
                "akid", "secret", None, None, "test",
            )))
            .time_source(time_source)
            .build();
        let signer = AuthTokenGenerator::new(
            Config::builder()
                .hostname("prod-instance.us-east-1.rds.amazonaws.com")
                .port(3306)
                .region(Region::new("us-east-1"))
                .username("peccy")
                .build()
                .unwrap(),
        );

        let signed_url = signer.auth_token(&sdk_config).await.unwrap();
        assert_eq!(signed_url.as_str(), "prod-instance.us-east-1.rds.amazonaws.com:3306/?Action=connect&DBUser=peccy&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=akid%2F20240826%2Fus-east-1%2Frds-db%2Faws4_request&X-Amz-Date=20240826T220000Z&X-Amz-Expires=900&X-Amz-SignedHeaders=host&X-Amz-Signature=dd0cba843009474347af724090233265628ace491ea17ce3eb3da098b983ad89");
    }
}
