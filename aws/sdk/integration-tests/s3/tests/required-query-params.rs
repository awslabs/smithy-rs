/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_sdk_s3::middleware::DefaultMiddleware;
use aws_sdk_s3::operation::AbortMultipartUpload;
use aws_sdk_s3::Region;
use aws_smithy_http::body::SdkBody;
use aws_smithy_http::operation::BuildError;

#[tokio::test]
async fn test_error_when_required_query_param_is_unset() {
    let conf = aws_sdk_s3::Config::builder()
        .region(Region::new("us-east-1"))
        .build();

    let err = AbortMultipartUpload::builder()
        .bucket("test-bucket")
        .key("test.txt")
        .build()
        .unwrap()
        .make_operation(&conf)
        .await
        .unwrap_err();

    assert!(matches!(
        BuildError::MissingField {
            field: "upload_id",
            details: "cannot be empty or unset",
        },
        err
    ))
}

#[tokio::test]
async fn test_error_when_required_query_param_is_set_but_empty() {
    let conf = aws_sdk_s3::Config::builder()
        .region(Region::new("us-east-1"))
        .build();
    let err = AbortMultipartUpload::builder()
        .bucket("test-bucket")
        .key("test.txt")
        .upload_id("")
        .build()
        .unwrap()
        .make_operation(&conf)
        .await
        .unwrap_err();

    assert!(matches!(
        BuildError::MissingField {
            field: "upload_id",
            details: "cannot be empty or unset",
        },
        err
    ))
}
