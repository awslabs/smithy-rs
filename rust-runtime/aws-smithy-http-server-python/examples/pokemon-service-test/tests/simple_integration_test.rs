/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

// Files here are for running integration tests.
// These tests only have access to your crate's public API.
// See: https://doc.rust-lang.org/book/ch11-03-test-organization.html#integration-tests

use aws_smithy_types::error::display::DisplayErrorContext;
use serial_test::serial;

use crate::helpers::{client, http2_client, PokemonClient, PokemonService};

mod helpers;

#[tokio::test]
#[serial]
async fn simple_integration_test() {
    let _program = PokemonService::run().await;
    simple_integration_test_with_client(client()).await;
}

#[tokio::test]
#[serial]
async fn simple_integration_test_http2() {
    let _program = PokemonService::run_http2().await;
    simple_integration_test_with_client(http2_client()).await;
}

async fn simple_integration_test_with_client(client: PokemonClient) {
    let service_statistics_out = client.get_server_statistics().send().await.unwrap();
    assert_eq!(0, service_statistics_out.calls_count.unwrap());

    let pokemon_species_output = client
        .get_pokemon_species()
        .name("pikachu")
        .send()
        .await
        .unwrap();
    assert_eq!("pikachu", pokemon_species_output.name().unwrap());

    let service_statistics_out = client.get_server_statistics().send().await.unwrap();
    assert_eq!(1, service_statistics_out.calls_count.unwrap());

    let pokemon_species_error = client
        .get_pokemon_species()
        .name("some_pokémon")
        .send()
        .await
        .unwrap_err();
    let message = DisplayErrorContext(pokemon_species_error).to_string();
    let expected =
        r#"ResourceNotFoundError [ResourceNotFoundException]: Requested Pokémon not available"#;
    assert!(
        message.contains(expected),
        "expected '{message}' to contain '{expected}'"
    );

    let service_statistics_out = client.get_server_statistics().send().await.unwrap();
    assert_eq!(2, service_statistics_out.calls_count.unwrap());

    let _health_check = client.check_health().send().await.unwrap();
}
