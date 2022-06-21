/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_http::user_agent::AwsUserAgent;
use aws_sdk_s3::{
    middleware::DefaultMiddleware,
    model::ChecksumAlgorithm,
    operation::{GetObject, PutObject},
    output::GetObjectOutput,
    Credentials, Region,
};
use aws_smithy_client::{
    test_connection::{capture_request, TestConnection},
    Client as CoreClient,
};
use aws_smithy_http::body::SdkBody;
use http::{HeaderValue, Uri};
use std::time::{Duration, UNIX_EPOCH};

// static INIT_LOGGER: std::sync::Once = std::sync::Once::new();
// fn init_logger() {
//     INIT_LOGGER.call_once(|| {
//         tracing_subscriber::fmt::init();
//     });
// }

pub type Client<C> = CoreClient<C, DefaultMiddleware>;

/// Test connection for the movies IT
/// headers are signed with actual creds, at some point we could replace them with verifiable test
/// credentials, but there are plenty of other tests that target signing
fn new_checksum_validated_response_test_connection(
    checksum_header_name: &'static str,
    checksum_header_value: &'static str,
) -> TestConnection<&'static str> {
    TestConnection::new(vec![
        (http::Request::builder()
             .header("x-amz-checksum-mode", "ENABLED")
             .header("user-agent", "aws-sdk-rust/0.123.test os/windows/XPSP3 lang/rust/1.50.0")
             .header("x-amz-date", "20210618T170728Z")
             .header("x-amz-content-sha256", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
             .header("x-amz-user-agent", "aws-sdk-rust/0.123.test api/test-service/0.123 os/windows/XPSP3 lang/rust/1.50.0")
             .header("authorization", "AWS4-HMAC-SHA256 Credential=ANOTREAL/20210618/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-checksum-mode;x-amz-content-sha256;x-amz-date;x-amz-security-token;x-amz-user-agent, Signature=eb9e58fa4fb04c8e6f160705017fdbb497ccff0efee4227b3a56f900006c3882")
             .uri(Uri::from_static("https://s3.us-east-1.amazonaws.com/some-test-bucket/test.txt?x-id=GetObject")).body(SdkBody::empty()).unwrap(),
         http::Response::builder()
             .header("x-amz-request-id", "4B4NGF0EAWN0GE63")
             .header("content-length", "11")
             .header("etag", "\"3e25960a79dbc69b674cd4ec67a72c62\"")
             .header(checksum_header_name, checksum_header_value)
             .header("content-type", "application/octet-stream")
             .header("server", "AmazonS3")
             .header("content-encoding", "")
             .header("last-modified", "Tue, 21 Jun 2022 16:29:14 GMT")
             .header("date", "Tue, 21 Jun 2022 16:29:23 GMT")
             .header("x-amz-id-2", "kPl+IVVZAwsN8ePUyQJZ40WD9dzaqtr4eNESArqE68GSKtVvuvCTDe+SxhTT+JTUqXB1HL4OxNM=")
             .header("accept-ranges", "bytes")
             .status(http::StatusCode::from_u16(200).unwrap())
             .body(r#"Hello world"#).unwrap()),
    ])
}

async fn test_checksum_on_streaming_response(
    checksum_header_name: &'static str,
    checksum_header_value: &'static str,
) -> GetObjectOutput {
    let creds = Credentials::new(
        "ANOTREAL",
        "notrealrnrELgWzOk3IfjzDKtFBhDby",
        Some("notarealsessiontoken".to_string()),
        None,
        "test",
    );
    let conf = aws_sdk_s3::Config::builder()
        .credentials_provider(creds)
        .region(Region::new("us-east-1"))
        .build();
    let conn = new_checksum_validated_response_test_connection(
        checksum_header_name,
        checksum_header_value,
    );
    let client = Client::new(conn.clone());

    let mut op = GetObject::builder()
        .bucket("some-test-bucket")
        .key("test.txt")
        .checksum_mode(aws_sdk_s3::model::ChecksumMode::Enabled)
        .build()
        .unwrap()
        .make_operation(&conf)
        .await
        .unwrap();
    op.properties_mut()
        .insert(UNIX_EPOCH + Duration::from_secs(1624036048));
    op.properties_mut().insert(AwsUserAgent::for_tests());

    let res = client.call(op).await.unwrap();

    conn.assert_requests_match(&[http::header::HeaderName::from_static("x-amz-checksum-mode")]);

    res
}

#[tokio::test]
async fn test_crc32_checksum_on_streaming_response() {
    let res = test_checksum_on_streaming_response("x-amz-checksum-crc32", "i9aeUg==").await;

    // Header checksums are base64 encoded
    assert_eq!(res.checksum_crc32(), Some("i9aeUg=="));
    let body = collect_body_into_string(res.body.into_inner()).await;

    assert_eq!(body, "Hello world");
}

#[tokio::test]
async fn test_crc32c_checksum_on_streaming_response() {
    let res = test_checksum_on_streaming_response("x-amz-checksum-crc32c", "crUfeA==").await;

    // Header checksums are base64 encoded
    assert_eq!(res.checksum_crc32_c(), Some("crUfeA=="));
    let body = collect_body_into_string(res.body.into_inner()).await;

    assert_eq!(body, "Hello world");
}

#[tokio::test]
async fn test_sha1_checksum_on_streaming_response() {
    let res =
        test_checksum_on_streaming_response("x-amz-checksum-sha1", "e1AsOh9IyGCa4hLN+2Od7jlnP14=")
            .await;

    // Header checksums are base64 encoded
    assert_eq!(res.checksum_sha1(), Some("e1AsOh9IyGCa4hLN+2Od7jlnP14="));
    let body = collect_body_into_string(res.body.into_inner()).await;

    assert_eq!(body, "Hello world");
}

#[tokio::test]
async fn test_sha256_checksum_on_streaming_response() {
    let res = test_checksum_on_streaming_response(
        "x-amz-checksum-sha256",
        "ZOyIygCyaOW6GjVnihtTFtIS9PNmskdyMlNKiuyjfzw=",
    )
    .await;

    // Header checksums are base64 encoded
    assert_eq!(
        res.checksum_sha256(),
        Some("ZOyIygCyaOW6GjVnihtTFtIS9PNmskdyMlNKiuyjfzw=")
    );
    let body = collect_body_into_string(res.body.into_inner()).await;

    assert_eq!(body, "Hello world");
}

// The test structure is identical for all supported checksum algorithms
async fn test_checksum_on_streaming_request(
    body: &'static [u8],
    checksum_algorithm: ChecksumAlgorithm,
    checksum_header_name: &'static str,
    expected_decoded_content_length: &'static str,
    expected_encoded_content_length: &'static str,
    expected_aws_chunked_encoded_body: &str,
) {
    let creds = aws_sdk_s3::Credentials::new(
        "ANOTREAL",
        "notrealrnrELgWzOk3IfjzDKtFBhDby",
        Some("notarealsessiontoken".to_string()),
        None,
        "test",
    );
    let conf = aws_sdk_s3::Config::builder()
        .credentials_provider(creds)
        .region(aws_sdk_s3::Region::new("us-east-1"))
        .build();
    let (conn, rcvr) = capture_request(None);

    let client: aws_smithy_client::Client<_, aws_sdk_s3::middleware::DefaultMiddleware> =
        aws_smithy_client::Client::new(conn.clone());

    let mut op = PutObject::builder()
        .bucket("test-bucket")
        .key("test.txt")
        .body(aws_sdk_s3::types::ByteStream::from_static(body))
        .checksum_algorithm(checksum_algorithm)
        .build()
        .unwrap()
        .make_operation(&conf)
        .await
        .expect("failed to construct operation");
    op.properties_mut()
        .insert(UNIX_EPOCH + Duration::from_secs(1624036048));
    op.properties_mut().insert(AwsUserAgent::for_tests());

    // The response from the fake connection won't return the expected XML but we don't care about
    // that error in this test
    let _ = client.call(op).await;
    let req = rcvr.expect_request();

    let headers = req.headers();
    let x_amz_content_sha256 = headers
        .get("x-amz-content-sha256")
        .expect("x-amz-content-sha256 header exists");
    let x_amz_trailer = headers
        .get("x-amz-trailer")
        .expect("x-amz-trailer header exists");
    let x_amz_decoded_content_length = headers
        .get("x-amz-decoded-content-length")
        .expect("x-amz-decoded-content-length header exists");
    let content_length = headers
        .get("Content-Length")
        .expect("Content-Length header exists");
    let content_encoding = headers
        .get("Content-Encoding")
        .expect("Content-Encoding header exists");

    assert_eq!(
        HeaderValue::from_static("STREAMING-UNSIGNED-PAYLOAD-TRAILER"),
        x_amz_content_sha256,
        "signing header is incorrect"
    );
    assert_eq!(
        HeaderValue::from_static(checksum_header_name),
        x_amz_trailer,
        "x-amz-trailer is incorrect"
    );
    assert_eq!(
        HeaderValue::from_static(aws_http::content_encoding::header_value::AWS_CHUNKED),
        content_encoding,
        "content-encoding wasn't set to aws-chunked"
    );

    // The length of the string "Hello world"
    assert_eq!(
        HeaderValue::from_static(expected_decoded_content_length),
        x_amz_decoded_content_length,
        "decoded content length was wrong"
    );
    // The sum of the length of the original body, chunk markers, and trailers
    assert_eq!(
        HeaderValue::from_static(expected_encoded_content_length),
        content_length,
        "content-length was wrong"
    );

    let body = collect_body_into_string(req.into_body()).await;
    // When sending a streaming body with a checksum, the trailers are included as part of the body content
    assert_eq!(body.as_str(), expected_aws_chunked_encoded_body,);
}

#[tokio::test]
async fn test_crc32_checksum_on_streaming_request() {
    test_checksum_on_streaming_request(
        b"Hello world",
        ChecksumAlgorithm::Crc32,
        "x-amz-checksum-crc32",
        "11",
        "52",
        "B\r\nHello world\r\n0\r\nx-amz-checksum-crc32:i9aeUg==\r\n\r\n",
    )
    .await
}

// This test isn't a duplicate. It tests CRC32C (note the C) checksum request validation
#[tokio::test]
async fn test_crc32c_checksum_on_streaming_request() {
    test_checksum_on_streaming_request(
        b"Hello world",
        ChecksumAlgorithm::Crc32C,
        "x-amz-checksum-crc32c",
        "11",
        "53",
        "B\r\nHello world\r\n0\r\nx-amz-checksum-crc32c:crUfeA==\r\n\r\n",
    )
    .await
}

#[tokio::test]
async fn test_sha1_checksum_on_streaming_request() {
    test_checksum_on_streaming_request(
        b"Hello world",
        ChecksumAlgorithm::Sha1,
        "x-amz-checksum-sha1",
        "11",
        "71",
        "B\r\nHello world\r\n0\r\nx-amz-checksum-sha1:e1AsOh9IyGCa4hLN+2Od7jlnP14=\r\n\r\n",
    )
    .await
}

#[tokio::test]
async fn test_sha256_checksum_on_streaming_request() {
    test_checksum_on_streaming_request(
        b"Hello world",
        ChecksumAlgorithm::Sha256,
        "x-amz-checksum-sha256",
        "11",
        "89",
        "B\r\nHello world\r\n0\r\nx-amz-checksum-sha256:ZOyIygCyaOW6GjVnihtTFtIS9PNmskdyMlNKiuyjfzw=\r\n\r\n",
    )
    .await
}

async fn collect_body_into_string(mut body: aws_smithy_http::body::SdkBody) -> String {
    use bytes::Buf;
    use bytes_utils::SegmentedBuf;
    use http_body::Body;
    use std::io::Read;

    let mut output = SegmentedBuf::new();
    while let Some(buf) = body.data().await {
        output.push(buf.unwrap());
    }

    let mut output_text = String::new();
    output
        .reader()
        .read_to_string(&mut output_text)
        .expect("Doesn't cause IO errors");

    output_text
}
