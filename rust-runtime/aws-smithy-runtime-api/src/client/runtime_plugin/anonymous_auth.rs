/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! The [AnonymousAuthRuntimePlugin] and supporting code.

use super::RuntimePlugin;
use crate::client::auth::option_resolver::{
    StaticAuthOptionResolver, StaticAuthOptionResolverParams,
};
use crate::client::auth::{AuthSchemeId, HttpAuthScheme, HttpAuthSchemes, HttpRequestSigner};
use crate::client::identity::anonymous::AnonymousIdentityResolver;
use crate::client::identity::{Identity, IdentityResolver, IdentityResolvers};
use crate::client::interceptors::Interceptors;
use crate::client::orchestrator::{ConfigBagAccessors, HttpRequest};
use crate::client::runtime_plugin::BoxError;
use crate::config_bag::ConfigBag;

const ANONYMOUS_AUTH_SCHEME_ID: AuthSchemeId = AuthSchemeId::new("anonymous");

/// A [RuntimePlugin] to provide anonymous authentication. This runtime plugin sets its own:
/// - [AuthOptionResolver]
/// - [AuthOptionResolverParams]
/// - [IdentityResolvers]
/// - [HttpAuthSchemes]
///
/// **The above components will replace any existing ones!** As such, don't use this plugin unless:
/// - You only need to make anonymous requests, such as when interacting with [Open Data](https://aws.amazon.com/opendata/).
/// - You're writing orchestrator tests and don't care about authentication.
pub struct AnonymousAuthRuntimePlugin;

impl RuntimePlugin for AnonymousAuthRuntimePlugin {
    fn configure(
        &self,
        cfg: &mut ConfigBag,
        _interceptors: &mut Interceptors,
    ) -> Result<(), BoxError> {
        cfg.set_auth_option_resolver_params(StaticAuthOptionResolverParams::new().into());
        cfg.set_auth_option_resolver(StaticAuthOptionResolver::new(vec![
            ANONYMOUS_AUTH_SCHEME_ID,
        ]));
        cfg.set_identity_resolvers(
            IdentityResolvers::builder()
                .identity_resolver(ANONYMOUS_AUTH_SCHEME_ID, AnonymousIdentityResolver::new())
                .build(),
        );
        cfg.set_http_auth_schemes(
            HttpAuthSchemes::builder()
                .auth_scheme(ANONYMOUS_AUTH_SCHEME_ID, AnonymousAuthScheme::new())
                .build(),
        );

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct AnonymousAuthScheme {
    signer: AnonymousSigner,
}

impl AnonymousAuthScheme {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Default)]
struct AnonymousSigner;

impl HttpRequestSigner for AnonymousSigner {
    fn sign_request(
        &self,
        _request: &mut HttpRequest,
        _identity: &Identity,
        _config_bag: &ConfigBag,
    ) -> Result<(), BoxError> {
        Ok(())
    }
}

impl HttpAuthScheme for AnonymousAuthScheme {
    fn scheme_id(&self) -> AuthSchemeId {
        ANONYMOUS_AUTH_SCHEME_ID
    }

    fn identity_resolver<'a>(
        &self,
        identity_resolvers: &'a IdentityResolvers,
    ) -> Option<&'a dyn IdentityResolver> {
        identity_resolvers.identity_resolver(ANONYMOUS_AUTH_SCHEME_ID)
    }

    fn request_signer(&self) -> &dyn HttpRequestSigner {
        &self.signer
    }
}
