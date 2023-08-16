/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use super::repr::{self, BaseProvider};
use crate::credential_process::CredentialProcessProvider;
use crate::profile::credentials::ProfileFileError;
use crate::provider_config::ProviderConfig;
use crate::sts;
use crate::web_identity_token::{StaticConfiguration, WebIdentityTokenCredentialsProvider};
use aws_credential_types::provider::{self, error::CredentialsError, ProvideCredentials};
use aws_sdk_sts::config::{Builder as StsConfigBuilder, Credentials};
use aws_sdk_sts::Client as StsClient;
use aws_smithy_async::time::SharedTimeSource;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug)]
pub(super) struct AssumeRoleProvider {
    role_arn: String,
    external_id: Option<String>,
    session_name: Option<String>,
    time_source: SharedTimeSource,
}

impl AssumeRoleProvider {
    pub(super) async fn credentials(
        &self,
        input_credentials: Credentials,
        sts_config: &StsConfigBuilder,
    ) -> provider::Result {
        let config = sts_config
            .clone()
            .credentials_provider(input_credentials)
            .build();
        let client = StsClient::from_conf(config);
        let session_name = &self.session_name.as_ref().cloned().unwrap_or_else(|| {
            sts::util::default_session_name("assume-role-from-profile", self.time_source.now())
        });
        let assume_role_creds = client
            .assume_role()
            .role_arn(&self.role_arn)
            .set_external_id(self.external_id.clone())
            .role_session_name(session_name)
            .send()
            .await
            .map_err(CredentialsError::provider_error)?
            .credentials;
        sts::util::into_credentials(assume_role_creds, "AssumeRoleProvider")
    }
}

#[derive(Debug)]
pub(super) struct ProviderChain {
    base: Arc<dyn ProvideCredentials>,
    chain: Vec<AssumeRoleProvider>,
}

impl ProviderChain {
    pub(crate) fn base(&self) -> &dyn ProvideCredentials {
        self.base.as_ref()
    }

    pub(crate) fn chain(&self) -> &[AssumeRoleProvider] {
        self.chain.as_slice()
    }
}

impl ProviderChain {
    pub(super) fn from_repr(
        provider_config: &ProviderConfig,
        repr: repr::ProfileChain<'_>,
        factory: &named::NamedProviderFactory,
    ) -> Result<Self, ProfileFileError> {
        let base = match repr.base() {
            BaseProvider::NamedSource(name) => {
                factory
                    .provider(name)
                    .ok_or(ProfileFileError::UnknownProvider {
                        name: name.to_string(),
                    })?
            }
            BaseProvider::AccessKey(key) => Arc::new(key.clone()),
            BaseProvider::CredentialProcess(credential_process) => Arc::new(
                CredentialProcessProvider::new(credential_process.unredacted().into()),
            ),
            BaseProvider::WebIdentityTokenRole {
                role_arn,
                web_identity_token_file,
                session_name,
            } => {
                let provider = WebIdentityTokenCredentialsProvider::builder()
                    .static_configuration(StaticConfiguration {
                        web_identity_token_file: web_identity_token_file.into(),
                        role_arn: role_arn.to_string(),
                        session_name: session_name.map(|sess| sess.to_string()).unwrap_or_else(
                            || {
                                sts::util::default_session_name(
                                    "web-identity-token-profile",
                                    provider_config.time_source().now(),
                                )
                            },
                        ),
                    })
                    .configure(provider_config)
                    .build();
                Arc::new(provider)
            }
            #[allow(unused_variables)]
            BaseProvider::Sso {
                sso_account_id,
                sso_region,
                sso_role_name,
                sso_start_url,
            } => {
                #[cfg(feature = "sso")]
                {
                    use crate::sso::{credentials::SsoProviderConfig, SsoCredentialsProvider};
                    use aws_types::region::Region;

                    let sso_config = SsoProviderConfig {
                        account_id: sso_account_id.to_string(),
                        role_name: sso_role_name.to_string(),
                        start_url: sso_start_url.to_string(),
                        region: Region::new(sso_region.to_string()),
                        // TODO(https://github.com/awslabs/aws-sdk-rust/issues/703): Implement sso_session_name profile property
                        session_name: None,
                    };
                    Arc::new(SsoCredentialsProvider::new(provider_config, sso_config))
                }
                #[cfg(not(feature = "sso"))]
                {
                    Err(ProfileFileError::FeatureNotEnabled {
                        feature: "sso".into(),
                    })?
                }
            }
        };
        tracing::info!(base = ?repr.base(), "first credentials will be loaded from {:?}", repr.base());
        let chain = repr
            .chain()
            .iter()
            .map(|role_arn| {
                tracing::info!(role_arn = ?role_arn, "which will be used to assume a role");
                AssumeRoleProvider {
                    role_arn: role_arn.role_arn.into(),
                    external_id: role_arn.external_id.map(|id| id.into()),
                    session_name: role_arn.session_name.map(|id| id.into()),
                    time_source: provider_config.time_source(),
                }
            })
            .collect();
        Ok(ProviderChain { base, chain })
    }
}

pub(super) mod named {
    use std::collections::HashMap;
    use std::sync::Arc;

    use aws_credential_types::provider::ProvideCredentials;
    use std::borrow::Cow;

    #[derive(Debug)]
    pub(crate) struct NamedProviderFactory {
        providers: HashMap<Cow<'static, str>, Arc<dyn ProvideCredentials>>,
    }

    fn lower_cow(mut input: Cow<'_, str>) -> Cow<'_, str> {
        if !input.chars().all(|c| c.is_ascii_lowercase()) {
            input.to_mut().make_ascii_lowercase();
        }
        input
    }

    impl NamedProviderFactory {
        pub(crate) fn new(
            providers: HashMap<Cow<'static, str>, Arc<dyn ProvideCredentials>>,
        ) -> Self {
            let providers = providers
                .into_iter()
                .map(|(k, v)| (lower_cow(k), v))
                .collect();
            Self { providers }
        }

        pub(crate) fn provider(&self, name: &str) -> Option<Arc<dyn ProvideCredentials>> {
            self.providers.get(&lower_cow(Cow::Borrowed(name))).cloned()
        }
    }
}

#[cfg(test)]
mod test {
    use crate::profile::credentials::exec::named::NamedProviderFactory;
    use crate::profile::credentials::exec::ProviderChain;
    use crate::profile::credentials::repr::{BaseProvider, ProfileChain};
    use crate::provider_config::ProviderConfig;
    use crate::test_case::no_traffic_connector;

    use aws_credential_types::Credentials;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn providers_case_insensitive() {
        let mut base = HashMap::new();
        base.insert(
            "Environment".into(),
            Arc::new(Credentials::for_tests()) as _,
        );
        let provider = NamedProviderFactory::new(base);
        assert!(provider.provider("environment").is_some());
        assert!(provider.provider("envIROnment").is_some());
        assert!(provider.provider(" envIROnment").is_none());
        assert!(provider.provider("Environment").is_some());
    }

    #[test]
    fn error_on_unknown_provider() {
        let factory = NamedProviderFactory::new(HashMap::new());
        let chain = ProviderChain::from_repr(
            &ProviderConfig::empty().with_http_connector(no_traffic_connector()),
            ProfileChain {
                base: BaseProvider::NamedSource("floozle"),
                chain: vec![],
            },
            &factory,
        );
        let err = chain.expect_err("no source by that name");
        assert!(
            format!("{}", err).contains(
                "profile referenced `floozle` provider but that provider is not supported"
            ),
            "`{}` did not match expected error",
            err
        );
    }
}
