/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_sdk_ec2::{client::Waiters, config::Region, error::DisplayErrorContext, Client};
use aws_smithy_async::test_util::tick_advance_sleep::{
    tick_advance_time_and_sleep, TickAdvanceTime,
};
use aws_smithy_runtime::{
    client::http::test_util::dvr::ReplayingClient, test_util::capture_test_logs::show_test_logs,
};
use aws_smithy_runtime_api::client::waiters::error::WaiterError;
use aws_smithy_types::retry::RetryConfig;
use std::time::Duration;

async fn prerequisites() -> (Client, ReplayingClient, TickAdvanceTime) {
    let (time_source, sleep_impl) = tick_advance_time_and_sleep();
    let client =
        ReplayingClient::from_file("tests/instance-status-ok-waiter-success.json").unwrap();
    let config = aws_sdk_ec2::Config::builder()
        .with_test_defaults()
        .http_client(client.clone())
        .time_source(time_source.clone())
        .sleep_impl(sleep_impl)
        .region(Region::new("us-west-2"))
        .retry_config(RetryConfig::standard())
        .build();
    (aws_sdk_ec2::Client::from_conf(config), client, time_source)
}

#[tokio::test]
async fn waiters_success() {
    let _logs = show_test_logs();

    let (ec2, http_client, time_source) = prerequisites().await;

    ec2.start_instances()
        .instance_ids("i-09fb4224219ac6902")
        .send()
        .await
        .unwrap();

    let waiter_task = tokio::spawn(
        ec2.wait_for_instance_status_ok()
            .instance_ids("i-09fb4224219ac6902")
            .wait(Duration::from_secs(300)),
    );

    time_source.tick(Duration::from_secs(305)).await;
    waiter_task.await.unwrap().unwrap();

    http_client.full_validate("application/xml").await.unwrap();
}

#[tokio::test]
async fn waiters_exceed_max_wait_time() {
    let _logs = show_test_logs();

    let (ec2, _, time_source) = prerequisites().await;

    ec2.start_instances()
        .instance_ids("i-09fb4224219ac6902")
        .send()
        .await
        .unwrap();

    let waiter_task = tokio::spawn(
        ec2.wait_for_instance_status_ok()
            .instance_ids("i-09fb4224219ac6902")
            .wait(Duration::from_secs(30)),
    );

    time_source.tick(Duration::from_secs(35)).await;
    let err = waiter_task.await.unwrap().err().expect("should fail");
    match err {
        WaiterError::ExceededMaxWait(context) => {
            assert_eq!(30, context.max_wait().as_secs());
            assert_eq!(30, context.elapsed().as_secs());
            assert_eq!(3, context.poll_count());
        }
        err => panic!("unexpected error: {}", DisplayErrorContext(&err)),
    }
}
