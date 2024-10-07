/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Code related to creating signed URLs for logging in to RDS.
//!
//! For more information, see https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/UsingWithRDS.IAMDBAuth.Connecting.html

use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::http_request;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::identity::Identity;
use aws_types::region::Region;
use std::fmt::Debug;
use std::time::Duration;

const ACTION: &str = "connect";
const SERVICE: &str = "rds-db";

/// A signer that generates an auth token for a database.
#[derive(Debug)]
pub struct Signer {
    config: SignerConfig,
}

impl Signer {
    /// Given a `SignerConfig`, create a new RDS database login URL signer.
    pub fn new(config: SignerConfig) -> Self {
        Self { config }
    }

    /// Return a signed URL usable as an auth token.
    pub async fn get_auth_token(
        &self,
        config: &aws_types::sdk_config::SdkConfig,
    ) -> Result<String, BoxError> {
        let credentials = self
            .config
            .credentials()
            .or(config.credentials_provider().as_ref())
            .ok_or("credentials are required to create an RDS login URL")?
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
        signing_settings.expires_in = Some(Duration::from_secs(900));
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
            url.query_pairs_mut().append_pair(name, &value);
        }

        let response = url.to_string().split_off("https://".len());

        Ok(response)
    }
}

/// Configuration for an RDS auth URL signer.
#[derive(Debug)]
pub struct SignerConfig {
    /// The AWS credentials to sign requests with. Uses the default credential provider chain if not specified.
    credentials: Option<SharedCredentialsProvider>,

    /// The hostname of the database to connect to.
    hostname: String,

    /// The port number the database is listening on.
    port: u32,

    /// The region the database is located in. Uses the region inferred from the runtime if omitted.
    region: Option<Region>,

    /// The username to login as.
    username: String,
}

impl SignerConfig {
    /// Create a new `SignerConfigBuilder`.
    pub fn builder() -> SignerConfigBuilder {
        SignerConfigBuilder::default()
    }

    /// The AWS credentials to sign requests with.
    pub fn credentials(&self) -> Option<&SharedCredentialsProvider> {
        self.credentials.as_ref()
    }

    /// The hostname of the database to connect to.
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// The port number the database is listening on.
    pub fn port(&self) -> u32 {
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
}

/// A builder for [`SignerConfig`]s.
#[derive(Debug, Default)]
pub struct SignerConfigBuilder {
    /// The AWS credentials to sign requests with. Uses the default credential provider chain if not specified.
    credentials: Option<SharedCredentialsProvider>,

    /// The hostname of the database to connect to.
    hostname: Option<String>,

    /// The port number the database is listening on.
    port: Option<u32>,

    /// The region the database is located in. Uses the region inferred from the runtime if omitted.
    region: Option<Region>,

    /// The database username to login as.
    username: Option<String>,
}

impl SignerConfigBuilder {
    /// Set the AWS credentials to sign requests with. Uses the default credential provider chain if not specified.
    pub fn credentials(mut self, credentials: SharedCredentialsProvider) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// The hostname of the database to connect to.
    pub fn hostname(mut self, hostname: impl Into<String>) -> Self {
        self.hostname = Some(hostname.into());
        self
    }

    /// The port number the database is listening on.
    pub fn port(mut self, port: u32) -> Self {
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

    /// Consume this builder, returning an error if required fields are missing.
    /// Otherwise, return a new `SignerConfig`.
    pub fn build(self) -> Result<SignerConfig, BoxError> {
        Ok(SignerConfig {
            credentials: self.credentials,
            hostname: self.hostname.ok_or("A hostname is required")?,
            port: self.port.ok_or("a port is required")?,
            region: self.region,
            username: self.username.ok_or("a username is required")?,
        })
    }
}

#[cfg(test)]
mod test {
    use super::{Signer, SignerConfig};
    use aws_credential_types::provider::SharedCredentialsProvider;
    use aws_credential_types::Credentials;
    use aws_smithy_async::test_util::ManualTimeSource;
    use aws_types::region::Region;
    use aws_types::SdkConfig;
    use std::time::SystemTime;

    #[tokio::test]
    async fn signing_works() {
        // Should generate the same result as running the following AWS CLI command:
        // aws rds generate-db-auth-token \
        // --hostname iamauth-databasecluster.cluster-abcdefg222hq.us-east-1.rds.amazonaws.com \
        // --port 3306 --username mydbuser --region us-east-1
        let time_source = ManualTimeSource::new(SystemTime::UNIX_EPOCH);
        let sdk_config = SdkConfig::builder()
            .credentials_provider(SharedCredentialsProvider::new(Credentials::for_tests()))
            .time_source(time_source)
            .build();
        let signer = Signer::new(
            SignerConfig::builder()
                .hostname("dontcare.fake-region-1.rds.amazonaws.com")
                .port(3306)
                .region(Region::new("fake-region-1"))
                .username("mydbuser")
                .build()
                .unwrap(),
        );

        let signed_url = signer.get_auth_token(&sdk_config).await.unwrap();
        assert_eq!(signed_url, "dontcare.fake-region-1.rds.amazonaws.com:3306/?Action=connect&DBUser=mydbuser&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=ANOTREAL%2F19700101%2Ffake-region-1%2Frds-db%2Faws4_request&X-Amz-Date=19700101T000000Z&X-Amz-Expires=900&X-Amz-SignedHeaders=host&X-Amz-Signature=32562254e5a540b51186f885e7df18188f5e963133f081c95668f8ce6d17c6c1");
    }
}
