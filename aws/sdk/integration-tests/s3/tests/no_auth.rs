/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_smithy_client::dvr::ReplayingConnection;
use aws_smithy_protocol_test::MediaType;
use aws_smithy_runtime::test_util::capture_test_logs::capture_test_logs;

#[tokio::test]
async fn list_objects() {
    let _logs = capture_test_logs();

    let conn = ReplayingConnection::from_file("tests/data/no_auth/list-objects.json").unwrap();
    let config = aws_config::from_env()
        .http_connector(conn.clone())
        .no_credentials()
        .region("us-east-1")
        .load()
        .await;
    let client = aws_sdk_s3::Client::new(&config);

    let result = client
        .list_objects()
        .bucket("gdc-organoid-pancreatic-phs001611-2-open")
        .max_keys(3)
        .customize()
        .await
        .unwrap()
        .remove_invocation_id_for_tests()
        .user_agent_for_tests()
        .send()
        .await;
    dbg!(result).expect("success");

    conn.validate_body_and_headers(None, MediaType::Xml)
        .await
        .unwrap();
}

#[tokio::test]
async fn list_objects_v2() {
    let _logs = capture_test_logs();

    let conn = ReplayingConnection::from_file("tests/data/no_auth/list-objects-v2.json").unwrap();
    let config = aws_config::from_env()
        .http_connector(conn.clone())
        .no_credentials()
        .region("us-east-1")
        .load()
        .await;
    let client = aws_sdk_s3::Client::new(&config);

    let result = client
        .list_objects_v2()
        .bucket("gdc-organoid-pancreatic-phs001611-2-open")
        .max_keys(3)
        .customize()
        .await
        .unwrap()
        .remove_invocation_id_for_tests()
        .user_agent_for_tests()
        .send()
        .await;
    dbg!(result).expect("success");

    conn.validate_body_and_headers(None, MediaType::Xml)
        .await
        .unwrap();
}

#[tokio::test]
async fn head_object() {
    let _logs = capture_test_logs();

    let conn = ReplayingConnection::from_file("tests/data/no_auth/head-object.json").unwrap();
    let config = aws_config::from_env()
        .http_connector(conn.clone())
        .no_credentials()
        .region("us-east-1")
        .load()
        .await;
    let client = aws_sdk_s3::Client::new(&config);

    let result = client
        .head_object()
        .bucket("gdc-organoid-pancreatic-phs001611-2-open")
        .key("0431cddc-a418-4a79-a34d-6c041394e8e4/a6ddcc84-8e4d-4c68-885c-2d51168eec97.FPKM-UQ.txt.gz")
        .customize()
        .await
        .unwrap()
        .remove_invocation_id_for_tests()
        .user_agent_for_tests()
        .send()
        .await;
    dbg!(result).expect("success");

    conn.validate_body_and_headers(None, MediaType::Xml)
        .await
        .unwrap();
}

#[tokio::test]
async fn get_object() {
    let _logs = capture_test_logs();

    let conn = ReplayingConnection::from_file("tests/data/no_auth/get-object.json").unwrap();
    let config = aws_config::from_env()
        .http_connector(conn.clone())
        .no_credentials()
        .region("us-east-1")
        .load()
        .await;
    let client = aws_sdk_s3::Client::new(&config);

    let result = client
        .get_object()
        .bucket("gdc-organoid-pancreatic-phs001611-2-open")
        .key("0431cddc-a418-4a79-a34d-6c041394e8e4/a6ddcc84-8e4d-4c68-885c-2d51168eec97.FPKM-UQ.txt.gz")
        .customize()
        .await
        .unwrap()
        .remove_invocation_id_for_tests()
        .user_agent_for_tests()
        .send()
        .await;
    dbg!(result).expect("success");

    conn.validate_body_and_headers(None, MediaType::Xml)
        .await
        .unwrap();
}
