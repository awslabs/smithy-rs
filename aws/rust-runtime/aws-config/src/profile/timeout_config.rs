/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Load timeout configuration properties from an AWS profile

use crate::profile::Profile;
use crate::provider_config::ProviderConfig;
use aws_smithy_types::timeout::{TimeoutConfigBuilder, TimeoutConfigError};
use aws_types::os_shim_internal::{Env, Fs};
use std::time::Duration;

const PROFILE_VAR_CONNECT_TIMEOUT: &str = "connect_timeout";
const PROFILE_VAR_TLS_NEGOTIATION_TIMEOUT: &str = "tls_negotiation_timeout";
const PROFILE_VAR_READ_TIMEOUT: &str = "read_timeout";
const PROFILE_VAR_API_CALL_ATTEMPT_TIMEOUT: &str = "api_call_attempt_timeout";
const PROFILE_VAR_API_CALL_TIMEOUT: &str = "api_call_timeout";

/// Load timeout configuration properties from a profile file
///
/// This provider will attempt to load AWS shared configuration, then read timeout configuration properties
/// from the active profile.
///
/// # Examples
///
/// **Sets the `connect_timeout` to 2 seconds
/// ```ini
/// [default]
/// connect_timeout = 2
/// ```
///
/// **Sets the `connect_timeout` to 2 seconds _if and only if_ the `other` profile is selected.
///
/// ```ini
/// [profile other]
/// connect_timeout = 2
/// ```
///
/// This provider is part of the [default timeout_config provider chain](crate::default_provider::timeout_config).
#[derive(Debug, Default)]
pub struct ProfileFileTimeoutConfigProvider {
    fs: Fs,
    env: Env,
    profile_override: Option<String>,
}

/// Builder for [ProfileFileTimeoutConfigProvider]
#[derive(Default)]
pub struct Builder {
    config: Option<ProviderConfig>,
    profile_override: Option<String>,
}

impl Builder {
    /// Override the configuration for this provider
    pub fn configure(mut self, config: &ProviderConfig) -> Self {
        self.config = Some(config.clone());
        self
    }

    /// Override the profile name used by the [ProfileFileTimeoutConfigProvider]
    pub fn profile_name(mut self, profile_name: impl Into<String>) -> Self {
        self.profile_override = Some(profile_name.into());
        self
    }

    /// Build a [ProfileFileTimeoutConfigProvider] from this builder
    pub fn build(self) -> ProfileFileTimeoutConfigProvider {
        let conf = self.config.unwrap_or_default();
        ProfileFileTimeoutConfigProvider {
            env: conf.env(),
            fs: conf.fs(),
            profile_override: self.profile_override,
        }
    }
}

impl ProfileFileTimeoutConfigProvider {
    /// Create a new [ProfileFileTimeoutConfigProvider]
    ///
    /// To override the selected profile, set the `AWS_PROFILE` environment variable or use the [Builder].
    pub fn new() -> Self {
        Self {
            fs: Fs::real(),
            env: Env::real(),
            profile_override: None,
        }
    }

    /// [Builder] to construct a [ProfileFileTimeoutConfigProvider]
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Attempt to create a new TimeoutConfigBuilder from a profile file.
    pub async fn timeout_config_builder(&self) -> Result<TimeoutConfigBuilder, TimeoutConfigError> {
        let profile = match super::parser::load(&self.fs, &self.env).await {
            Ok(profile) => profile,
            Err(err) => {
                tracing::warn!(err = %err, "failed to parse profile");
                // return an empty builder
                return Ok(Default::default());
            }
        };

        let selected_profile = self
            .profile_override
            .as_deref()
            .unwrap_or_else(|| profile.selected_profile());
        let selected_profile = match profile.get_profile(selected_profile) {
            Some(profile) => profile,
            None => {
                tracing::warn!("failed to get selected '{}' profile", selected_profile);
                // return an empty builder
                return Ok(TimeoutConfigBuilder::default());
            }
        };

        let connect_timeout =
            construct_timeout_from_profile_var(selected_profile, PROFILE_VAR_CONNECT_TIMEOUT)?;
        let tls_negotiation_timeout = construct_timeout_from_profile_var(
            selected_profile,
            PROFILE_VAR_TLS_NEGOTIATION_TIMEOUT,
        )?;
        let read_timeout =
            construct_timeout_from_profile_var(selected_profile, PROFILE_VAR_READ_TIMEOUT)?;
        let api_call_attempt_timeout = construct_timeout_from_profile_var(
            selected_profile,
            PROFILE_VAR_API_CALL_ATTEMPT_TIMEOUT,
        )?;
        let api_call_timeout =
            construct_timeout_from_profile_var(selected_profile, PROFILE_VAR_API_CALL_TIMEOUT)?;

        let mut builder = TimeoutConfigBuilder::new();
        builder
            .set_connect_timeout(connect_timeout)
            .set_tls_negotiation_timeout(tls_negotiation_timeout)
            .set_read_timeout(read_timeout)
            .set_api_call_attempt_timeout(api_call_attempt_timeout)
            .set_api_call_timeout(api_call_timeout);

        Ok(builder)
    }
}

const SET_BY: &str = "aws profile";

fn construct_timeout_from_profile_var(
    profile: &Profile,
    var: &str,
) -> Result<Option<Duration>, TimeoutConfigError> {
    // TODO do I really need to clone this?
    let var = var.to_owned();
    match profile.get(&var) {
        Some(timeout) => match timeout.parse::<f32>() {
            Ok(timeout) if timeout < 0.0 => Err(TimeoutConfigError::InvalidTimeout {
                set_by: SET_BY.into(),
                name: var.into(),
                reason: "timeout must not be negative".into(),
            }),
            Ok(timeout) if timeout.is_nan() => Err(TimeoutConfigError::InvalidTimeout {
                set_by: SET_BY.into(),
                name: var.into(),
                reason: "timeout must not be NaN".into(),
            }),
            Ok(timeout) if timeout.is_infinite() => Err(TimeoutConfigError::InvalidTimeout {
                set_by: SET_BY.into(),
                name: var.into(),
                reason: "timeout must not be infinite".into(),
            }),
            Ok(timeout) => Ok(Some(Duration::from_secs_f32(timeout))),
            Err(_) => Err(TimeoutConfigError::CouldntParseTimeout {
                set_by: SET_BY.into(),
                name: var.into(),
            }),
        },
        None => Ok(None),
    }
}
