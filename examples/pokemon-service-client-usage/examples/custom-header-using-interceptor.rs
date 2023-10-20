/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */
/// In this example, a custom header x-amzn-client-ttl-seconds is set for all outgoing requests.
/// It serves as a demonstration of how an operation name can be retrieved and utilized within
/// the interceptor.
///
/// The example assumes that the Pokemon service is running on the localhost on TCP port 13734.
/// Refer to the [README.md](https://github.com/awslabs/smithy-rs/tree/main/examples/pokemon-service-client-usage/README.md)
/// file for instructions on how to launch the service locally.
///
/// The example can be run using `cargo run --example custom-header-using-interceptor`.
///
use std::{collections::HashMap, time::Duration};

use aws_smithy_types::config_bag::ConfigBag;
use pokemon_service_client::Client as PokemonClient;
use pokemon_service_client::{
    config::{interceptors::BeforeTransmitInterceptorContextMut, Interceptor, RuntimeComponents},
    error::BoxError,
};
use pokemon_service_client_usage::{setup_tracing_subscriber, ResultExt};
use tracing::info;

// URL where example Pokemon service is running.
static BASE_URL: &str = "http://localhost:13734";
// Header to send with each operation.
const HEADER_TO_SEND: hyper::header::HeaderName =
    hyper::header::HeaderName::from_static("x-amzn-client-ttl-seconds");

// The TtlHeaderInterceptor keeps a map of operation specific value to send
// in the header for each Request.
#[derive(Debug, Clone)]
pub struct TtlHeaderInterceptor {
    /// Default time-to-live for an operation.
    default_ttl: hyper::http::HeaderValue,
    /// Operation specific time-to-live.
    operation_ttl: HashMap<&'static str, hyper::http::HeaderValue>,
}

// Helper function to format duration as fractional seconds.
fn format_ttl_value(ttl: Duration) -> String {
    format!("{:.2}", ttl.as_secs_f64())
}

impl TtlHeaderInterceptor {
    fn new(default_ttl: Duration) -> Self {
        let duration_str = format_ttl_value(default_ttl);
        let default_ttl_value = hyper::http::HeaderValue::from_str(duration_str.as_str())
            .expect("could not create a header value for the default ttl");

        Self {
            default_ttl: default_ttl_value,
            operation_ttl: Default::default(),
        }
    }

    /// Adds an operation name specific timeout value that needs to be set in the header.
    fn add_operation_ttl(&mut self, operation_name: &'static str, ttl: Duration) {
        let duration_str = format_ttl_value(ttl);

        self.operation_ttl.insert(
            operation_name,
            hyper::http::HeaderValue::from_str(duration_str.as_str())
                .expect("cannot create header value for the given ttl duration"),
        );
    }
}

/// Appends the header `x-amzn-ttl-secs` using either the default time-to-live value
/// or an operation-specific value if it was set earlier using `add_operation_ttl`.
impl Interceptor for TtlHeaderInterceptor {
    fn name(&self) -> &'static str {
        "TtlHeaderInterceptor"
    }

    /// Before the request is signed, add the header to the outgoing request.
    fn modify_before_signing(
        &self,
        context: &mut BeforeTransmitInterceptorContextMut<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        // Metadata in the ConfigBag has the operation name.
        let metadata = cfg
            .load::<aws_smithy_http::operation::Metadata>()
            .expect("metadata should exist");
        let operation_name = metadata.name().to_string();

        // Get operation specific or default HeaderValue to set for the header key.
        let ttl = match self.operation_ttl.get(operation_name.as_str()) {
            Some(ttl) => ttl,
            None => &self.default_ttl,
        };

        context
            .request_mut()
            .headers_mut()
            .insert(&HEADER_TO_SEND, ttl.clone());

        info!("{operation_name} header set to {ttl:?}");

        Ok(())
    }
}

/// Creates a new Smithy client that is configured to communicate with a locally running Pokemon service on TCP port 13734.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// let client = create_client();
/// ```
fn create_client() -> PokemonClient {
    // By default set the value of all operations to 6.0
    static DEFAULT_TTL: Duration = Duration::from_secs(6);

    // Setup the interceptor to add an operation specific value of 3.5 secs to be added
    // for GetStorage operation.
    let mut ttl_headers_interceptor = TtlHeaderInterceptor::new(DEFAULT_TTL);
    ttl_headers_interceptor.add_operation_ttl("GetStorage", Duration::from_millis(3500));

    // The generated client has a type config::Builder that can be used to build a Config, which
    // allows configuring endpoint-resolver, timeouts, retries etc.
    let config = pokemon_service_client::Config::builder()
        .endpoint_resolver(BASE_URL)
        .interceptor(ttl_headers_interceptor)
        .build();

    pokemon_service_client::Client::from_conf(config)
}

#[tokio::main]
async fn main() {
    setup_tracing_subscriber();

    // Create a configured Smithy client.
    let client = create_client();

    // Call an operation `get_server_statistics` on Pokemon service.
    let response = client
        .get_server_statistics()
        .send()
        .await
        .custom_expect_and_log("get_server_statistics failed");

    info!(%BASE_URL, ?response, "Response for get_server_statistics()");

    // Call the operation `get_storage` on Pokemon service. The AddHeader middleware
    // will add a specific header name / value pair for this operation.
    let response = client
        .get_storage()
        .user("ash")
        .passcode("pikachu123")
        .send()
        .await
        .custom_expect_and_log("get_storage failed");

    // Print the response received from the service.
    info!(%BASE_URL, ?response, "Response received");
}
