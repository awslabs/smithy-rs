/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Code related to creating signed URLs for logging in to DSQL.

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

const ACTION: &str = "DbConnect";
const ACTION_ADMIN: &str = "DbConnectAdmin";
const SERVICE: &str = "dsql";

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
///            .hostname("peccy.dsql.us-east-1.on.aws")
///            .build()
///            .expect("cfg is valid"),
///    );
///    let token = generator.db_connect_admin_auth_token(&cfg).await.unwrap();
///    println!("{token}");
/// }
/// ```
#[derive(Debug)]
pub struct AuthTokenGenerator {
    config: Config,
}

/// An auth token usable as a password for a DSQL database.
///
/// This struct can be converted into a `&str` by calling `as_str`
/// or converted into a `String` by calling `to_string()`.
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
    /// Given a `Config`, create a new DSQL database login URL signer.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Return a signed URL usable as an auth token.
    pub async fn db_connect_auth_token(
        &self,
        config: &aws_types::sdk_config::SdkConfig,
    ) -> Result<AuthToken, BoxError> {
        self.inner(config, ACTION).await
    }

    /// Return a signed URL usable as an admin auth token.
    pub async fn db_connect_admin_auth_token(
        &self,
        config: &aws_types::sdk_config::SdkConfig,
    ) -> Result<AuthToken, BoxError> {
        self.inner(config, ACTION_ADMIN).await
    }

    async fn inner(
        &self,
        config: &aws_types::sdk_config::SdkConfig,
        action: &str,
    ) -> Result<AuthToken, BoxError> {
        let credentials = self
            .config
            .credentials()
            .or(config.credentials_provider())
            .ok_or("credentials are required to create a signed URL for DSQL")?
            .provide_credentials()
            .await?;
        let identity: Identity = credentials.into();
        let region = self
            .config
            .region()
            .or(config.region())
            .ok_or("a region is required")?;
        let time = config.time_source().ok_or("a time source is required")?;

        let mut signing_settings = SigningSettings::default();
        signing_settings.expires_in =
            Some(Duration::from_secs(self.config.expires_in().unwrap_or(900)));
        signing_settings.signature_location = http_request::SignatureLocation::QueryParams;

        let signing_params = v4::SigningParams::builder()
            .identity(&identity)
            .region(region.as_ref())
            .name(SERVICE)
            .time(time.now())
            .settings(signing_settings)
            .build()?;

        let url = format!("https://{}/?Action={}", self.config.hostname(), action);
        let signable_request =
            SignableRequest::new("GET", &url, std::iter::empty(), SignableBody::Bytes(&[]))
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

/// Configuration for a DSQL auth URL signer.
#[derive(Debug, Clone)]
pub struct Config {
    /// The AWS credentials to sign requests with.
    ///
    /// Uses the default credential provider chain if not specified.
    credentials: Option<SharedCredentialsProvider>,

    /// The hostname of the database to connect to.
    hostname: String,

    /// The region the database is located in. Uses the region inferred from the runtime if omitted.
    region: Option<Region>,

    /// The number of seconds the signed URL should be valid for.
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

    /// The region to sign requests with.
    pub fn region(&self) -> Option<&Region> {
        self.region.as_ref()
    }

    /// The number of seconds the signed URL should be valid for.
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

    /// The region the database is located in. Uses the region inferred from the runtime if omitted.
    region: Option<Region>,

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

    /// The region the database is located in.
    pub fn region(mut self, region: Region) -> Self {
        self.region = Some(region);
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
            region: self.region,
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
        let time_source = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(1724716800));
        let sdk_config = SdkConfig::builder()
            .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
                "akid", "secret", None, None, "test",
            )))
            .time_source(time_source)
            .build();
        let signer = AuthTokenGenerator::new(
            Config::builder()
                .hostname("peccy.dsql.us-east-1.on.aws")
                .region(Region::new("us-east-1"))
                .expires_in(450)
                .build()
                .unwrap(),
        );

        let signed_url = signer.db_connect_auth_token(&sdk_config).await.unwrap();
        assert_eq!(signed_url.as_str(), "peccy.dsql.us-east-1.on.aws/?Action=DbConnect&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=akid%2F20240827%2Fus-east-1%2Fdsql%2Faws4_request&X-Amz-Date=20240827T000000Z&X-Amz-Expires=450&X-Amz-SignedHeaders=host&X-Amz-Signature=f5f2ad764ca5df44045d4ab6ccecba0eef941b0007e5765885a0b6ed3702a3f8");
    }

    #[tokio::test]
    async fn signing_works_admin() {
        let time_source = ManualTimeSource::new(UNIX_EPOCH + Duration::from_secs(1724716800));
        let sdk_config = SdkConfig::builder()
            .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
                "akid", "secret", None, None, "test",
            )))
            .time_source(time_source)
            .build();
        let signer = AuthTokenGenerator::new(
            Config::builder()
                .hostname("peccy.dsql.us-east-1.on.aws")
                .region(Region::new("us-east-1"))
                .expires_in(450)
                .build()
                .unwrap(),
        );

        let signed_url = signer
            .db_connect_admin_auth_token(&sdk_config)
            .await
            .unwrap();
        assert_eq!(signed_url.as_str(), "peccy.dsql.us-east-1.on.aws/?Action=DbConnectAdmin&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=akid%2F20240827%2Fus-east-1%2Fdsql%2Faws4_request&X-Amz-Date=20240827T000000Z&X-Amz-Expires=450&X-Amz-SignedHeaders=host&X-Amz-Signature=267cf8d04d84444f7a62d5bdb40c44bfc6cb13dd6c64fa7f772df6bbaa90fff1");
    }
}
