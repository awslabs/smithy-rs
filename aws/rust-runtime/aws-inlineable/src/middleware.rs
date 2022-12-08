/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Base Middleware Stack

use aws_endpoint::AwsAuthStage;
use aws_http::auth::CredentialsStage;
use aws_http::recursion_detection::RecursionDetectionStage;
use aws_http::user_agent::UserAgentStage;
use aws_sig_auth::middleware::SigV4SigningStage;
use aws_sig_auth::signer::SigV4Signer;
use aws_smithy_http::endpoint::middleware::SmithyEndpointStage;
use aws_smithy_http_tower::map_request::{AsyncMapRequestLayer, MapRequestLayer};
use std::fmt::Debug;
use tower::ServiceBuilder;

/// Macro to generate the tower stack type. Arguments should be in reverse order
macro_rules! stack_type {
    ($first: ty, $($rest:ty),+) => {
        tower::layer::util::Stack<$first, stack_type!($($rest),+)>
    };
    ($only: ty) => {
        tower::layer::util::Stack<$only, tower::layer::util::Identity>
    }
}

// Note: the layers here appear in reverse order
type DefaultMiddlewareStack = stack_type!(
    MapRequestLayer<RecursionDetectionStage>,
    MapRequestLayer<SigV4SigningStage>,
    AsyncMapRequestLayer<CredentialsStage>,
    MapRequestLayer<UserAgentStage>,
    MapRequestLayer<AwsAuthStage>,
    MapRequestLayer<SmithyEndpointStage>
);

/// AWS Middleware Stack
///
/// This implements the middleware stack for this service. It will:
/// 1. Load credentials asynchronously into the property bag
/// 2. Sign the request with SigV4
/// 3. Resolve an Endpoint for the request
/// 4. Add a user agent to the request
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct DefaultMiddleware;

impl DefaultMiddleware {
    /// Create a new `DefaultMiddleware` stack
    ///
    /// Note: `DefaultMiddleware` holds no state.
    pub fn new() -> Self {
        DefaultMiddleware::default()
    }
}

// define the middleware stack in a non-generic location to reduce code bloat.
fn base() -> ServiceBuilder<DefaultMiddlewareStack> {
    let credential_provider = AsyncMapRequestLayer::for_mapper(CredentialsStage::new());
    let signer = MapRequestLayer::for_mapper(SigV4SigningStage::new(SigV4Signer::new()));
    let endpoint_stage = MapRequestLayer::for_mapper(SmithyEndpointStage::new());
    let auth_stage = MapRequestLayer::for_mapper(AwsAuthStage);
    let user_agent = MapRequestLayer::for_mapper(UserAgentStage::new());
    let recursion_detection = MapRequestLayer::for_mapper(RecursionDetectionStage::new());
    // These layers can be considered as occurring in order, that is:
    // 1. Resolve an endpoint
    // 2. Add a user agent
    // 3. Acquire credentials
    // 4. Sign with credentials
    // (5. Dispatch over the wire)
    ServiceBuilder::new()
        .layer(endpoint_stage)
        .layer(auth_stage)
        .layer(user_agent)
        .layer(credential_provider)
        .layer(signer)
        .layer(recursion_detection)
}

impl<S> tower::Layer<S> for DefaultMiddleware {
    type Service = <DefaultMiddlewareStack as tower::Layer<S>>::Service;

    fn layer(&self, inner: S) -> Self::Service {
        base().service(inner)
    }
}
