/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_config::SdkConfig;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_http::body::SdkBody;
use aws_smithy_runtime::client::http::test_util::{ConnectionEvent, EventClient};
use std::time::{Duration, UNIX_EPOCH};

#[tokio::test]
async fn test_signer() {
    let http_client = EventClient::new(vec![ConnectionEvent::new(
        http::Request::builder()
            .header("authorization", "AWS4-HMAC-SHA256 Credential=ANOTREAL/20210618/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token;x-amz-user-agent, Signature=ae78f74d26b6b0c3a403d9e8cc7ec3829d6264a2b33db672bf2b151bbb901786")
            .uri("https://test-bucket.s3.us-east-1.amazonaws.com/?list-type=2&prefix=prefix~")
            .body(SdkBody::empty())
            .unwrap(),
        http::Response::builder().status(200).body(SdkBody::empty()).unwrap(),
    )], TokioSleep::new());
    let sdk_config = SdkConfig::builder()
        .credentials_provider(SharedCredentialsProvider::new(
            Credentials::for_tests_with_session_token(),
        ))
        .region(Region::new("us-east-1"))
        .http_client(http_client.clone())
        .build();
    let client = Client::new(&sdk_config);
    let _ = client
        .list_objects_v2()
        .bucket("test-bucket")
        .prefix("prefix~")
        .customize()
        .await
        .unwrap()
        .request_time_for_tests(UNIX_EPOCH + Duration::from_secs(1624036048))
        .user_agent_for_tests()
        .send()
        .await;

    http_client.assert_requests_match(&[]);
}
