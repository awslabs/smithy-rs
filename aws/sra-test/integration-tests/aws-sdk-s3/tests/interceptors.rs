/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

mod util;

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use aws_smithy_client::dvr;
use aws_smithy_client::dvr::MediaType;
use aws_smithy_client::erase::DynConnector;
use aws_smithy_runtime_api::client::interceptors::context::phase::BeforeTransmit;
use aws_smithy_runtime_api::client::interceptors::{Interceptor, InterceptorContext};
use aws_smithy_runtime_api::client::orchestrator::ConfigBagAccessors;
use aws_smithy_runtime_api::client::orchestrator::RequestTime;
use aws_smithy_runtime_api::config_bag::ConfigBag;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LIST_BUCKETS_PATH: &str = "test-data/list-objects-v2.json";

#[tokio::test]
async fn operation_interceptor_test() {
    tracing_subscriber::fmt::init();

    let conn = dvr::ReplayingConnection::from_file(LIST_BUCKETS_PATH).unwrap();

    // Not setting `TestUserAgentInterceptor` here, expecting it to be set later by the
    // operation-level config.
    let config = aws_sdk_s3::Config::builder()
        .credentials_provider(Credentials::for_tests())
        .region(Region::new("us-east-1"))
        .http_connector(DynConnector::new(conn.clone()))
        .build();
    let client = Client::from_conf(config);
    let fixup = util::FixupPlugin {
        timestamp: UNIX_EPOCH + Duration::from_secs(1624036048),
    };

    let resp = dbg!(
        client
            .list_objects_v2()
            .bucket("test-bucket")
            .prefix("prefix~")
            .customize()
            .await
            .unwrap()
            .interceptor(util::TestUserAgentInterceptor)
            .send_orchestrator_with_plugin(Some(fixup))
            .await
    );
    let resp = resp.expect("valid e2e test");
    assert_eq!(resp.name(), Some("test-bucket"));
    conn.full_validate(MediaType::Xml).await.expect("success")
}

#[derive(Debug)]
struct RequestTimeResetInterceptor;
impl Interceptor for RequestTimeResetInterceptor {
    fn modify_before_signing(
        &self,
        _context: &mut InterceptorContext<BeforeTransmit>,
        cfg: &mut ConfigBag,
    ) -> Result<(), aws_smithy_runtime_api::client::interceptors::BoxError> {
        cfg.set_request_time(RequestTime::new(UNIX_EPOCH));

        Ok(())
    }
}

#[derive(Debug)]
struct RequestTimeAdvanceInterceptor(Duration);
impl Interceptor for RequestTimeAdvanceInterceptor {
    fn modify_before_signing(
        &self,
        _context: &mut InterceptorContext<BeforeTransmit>,
        cfg: &mut ConfigBag,
    ) -> Result<(), aws_smithy_runtime_api::client::interceptors::BoxError> {
        let request_time = cfg.request_time().unwrap();
        let request_time = RequestTime::new(request_time.system_time() + self.0);
        cfg.set_request_time(request_time);

        Ok(())
    }
}

#[tokio::test]
async fn interceptor_priority() {
    let conn = dvr::ReplayingConnection::from_file(LIST_BUCKETS_PATH).unwrap();

    // `RequestTimeResetInterceptor` will reset a `RequestTime` to `UNIX_EPOCH`, whose previous
    // value should be `SystemTime::now()` set by `FixupPlugin`.
    let config = aws_sdk_s3::Config::builder()
        .credentials_provider(Credentials::for_tests())
        .region(Region::new("us-east-1"))
        .http_connector(DynConnector::new(conn.clone()))
        .interceptor(util::TestUserAgentInterceptor)
        .interceptor(RequestTimeResetInterceptor)
        .build();
    let client = Client::from_conf(config);
    let fixup = util::FixupPlugin {
        timestamp: SystemTime::now(),
    };

    // `RequestTimeAdvanceInterceptor` configured at the operation level should run after,
    // expecting the `RequestTime` to move forward by the specified amount since `UNIX_EPOCH`.
    let resp = dbg!(
        client
            .list_objects_v2()
            .bucket("test-bucket")
            .prefix("prefix~")
            .customize()
            .await
            .unwrap()
            .interceptor(RequestTimeAdvanceInterceptor(Duration::from_secs(
                1624036048
            )))
            .send_orchestrator_with_plugin(Some(fixup))
            .await
    );
    let resp = resp.expect("valid e2e test");
    assert_eq!(resp.name(), Some("test-bucket"));
    conn.full_validate(MediaType::Xml).await.expect("success")
}

#[tokio::test]
async fn set_test_user_agent_through_request_mutation() {
    let conn = dvr::ReplayingConnection::from_file(LIST_BUCKETS_PATH).unwrap();

    let config = aws_sdk_s3::Config::builder()
        .credentials_provider(Credentials::for_tests())
        .region(Region::new("us-east-1"))
        .http_connector(DynConnector::new(conn.clone()))
        .build();
    let client = Client::from_conf(config);
    let fixup = util::FixupPlugin {
        timestamp: UNIX_EPOCH + Duration::from_secs(1624036048),
    };

    let resp = dbg!(
        client
            .list_objects_v2()
            .bucket("test-bucket")
            .prefix("prefix~")
            .customize()
            .await
            .unwrap()
            .mutate_request(|request| {
                request.headers_mut()
                    .insert(
                        http::HeaderName::from_static("user-agent"),
                        http::HeaderValue::from_str("aws-sdk-rust/0.123.test os/windows/XPSP3 lang/rust/1.50.0").unwrap(),
                    );
                request.headers_mut()
                    .insert(
                        http::HeaderName::from_static("x-amz-user-agent"),
                        http::HeaderValue::from_str("aws-sdk-rust/0.123.test api/test-service/0.123 os/windows/XPSP3 lang/rust/1.50.0").unwrap(),
                    );
            })
            .send_orchestrator_with_plugin(Some(fixup))
            .await
    );
    let resp = resp.expect("valid e2e test");
    assert_eq!(resp.name(), Some("test-bucket"));
    conn.full_validate(MediaType::Xml).await.expect("success")
}
