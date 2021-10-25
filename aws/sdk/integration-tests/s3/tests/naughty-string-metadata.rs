/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use aws_http::user_agent::AwsUserAgent;
use aws_sdk_s3::{operation::PutObject, Credentials, Region};
use aws_smithy_client::test_connection::capture_request;
use http::HeaderValue;
use std::time::UNIX_EPOCH;
use tokio::time::Duration;

const NAUGHTY_STRINGS: &str = include_str!("blns/blns.txt");

// // A useful way to find leaks in the signing system that requires an actual S3 bucket to test with
// // If you want to use this, update the credentials to be your credentials and change the bucket name
// // to your bucket
// #[tokio::test]
// async fn test_metadata_field_against_naughty_strings_list() -> Result<(), aws_sdk_s3::Error> {
//     // re-add `aws-config = { path = "../../build/aws-sdk/aws-config" }` to this project's Cargo.toml
//
//     let config = aws_config::load_from_env().await;
//     let client = aws_sdk_s3::Client::new(&config);
//
//     let mut req = client
//         .put_object()
//         .bucket("your-test-bucket-goes-here")
//         .key("test.txt")
//         .body(aws_sdk_s3::ByteStream::from_static(b"some test text"));
//
//     for (idx, line) in NAUGHTY_STRINGS.split('\n').enumerate() {
//         // add lines to metadata unless they're a comment or empty
//         // Some naughty strings aren't valid HeaderValues so we skip those too
//         if !line.starts_with("#") && !line.is_empty() && HeaderValue::from_str(line).is_ok() {
//             let key = format!("line-{}", idx);
//
//             req = req.metadata(key, line);
//         }
//     }
//
//     // If this fails due to signing then the signer choked on a bad string. To find out which string,
//     // send one request per line instead of adding all lines as metadata for one request.
//     let _ = req.send().await.unwrap();
//
//     Ok(())
// }

#[tokio::test]
async fn test_s3_signer_with_naughty_string_metadata() -> Result<(), aws_sdk_s3::Error> {
    let creds = Credentials::from_keys(
        "ANOTREAL",
        "notrealrnrELgWzOk3IfjzDKtFBhDby",
        Some("notarealsessiontoken".to_string()),
    );
    let conf = aws_sdk_s3::Config::builder()
        .credentials_provider(creds)
        .region(Region::new("us-east-1"))
        .build();
    let (conn, rcvr) = capture_request(None);

    let client = aws_hyper::Client::new(conn.clone());
    let mut builder = PutObject::builder()
        .bucket("test-bucket")
        .key("text.txt")
        .body(aws_sdk_s3::ByteStream::from_static(b"some test text"));

    for (idx, line) in NAUGHTY_STRINGS.split('\n').enumerate() {
        // add lines to metadata unless they're a comment or empty
        // Some naughty strings aren't valid HeaderValues so we skip those too
        if !line.starts_with("#") && !line.is_empty() && HeaderValue::from_str(line).is_ok() {
            let key = format!("line-{}", idx);

            builder = builder.metadata(key, line);
        }
    }

    let mut op = builder
        .build()
        .unwrap()
        .make_operation(&conf)
        .await
        .unwrap();
    op.properties_mut()
        .insert(UNIX_EPOCH + Duration::from_secs(1624036048));
    op.properties_mut().insert(AwsUserAgent::for_tests());

    client.call(op).await.unwrap();

    let expected_req = rcvr.expect_request();
    let auth_header = expected_req.headers().get("Authorization").unwrap();

    // This is a snapshot test taken from a known working test result
    assert!(auth_header
        .to_str()
        .unwrap()
        .contains("Signature=ec1b206cc8c5f9e05f583516521e1412a2c555b81fad011be661199612c53cb7"));

    Ok(())
}
