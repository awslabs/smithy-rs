//! Pokémon Service
//!
//! This crate implements the Pokémon Service.
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
use std::{convert::TryInto, sync::Arc};

use aws_smithy_http_server::Extension;
use pokemon_sdk::{error, input, model, output};
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::{prelude::*, EnvFilter};

const PIKACHU_ENGLISH_FLAVOR_TEXT: &str =
    "When several of these Pokémon gather, their electricity could build and cause lightning storms.";
const PIKACHU_SPANISH_FLAVOR_TEXT: &str =
    "Cuando varios de estos Pokémon se juntan, su energía puede causar fuertes tormentas.";
const PIKACHU_ITALIAN_FLAVOR_TEXT: &str =
    "Quando vari Pokémon di questo tipo si radunano, la loro energia può causare forti tempeste.";

/// Setup `tracing::subscriber` to read the log level from RUST_LOG environment variable.
pub fn setup_tracing() {
    let format = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_line_number(true)
        .with_level(true);
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
    tracing_subscriber::registry().with(format).with(filter).init();
}

/// PokémonService shared state.
///
/// Some application may want to manage state between handlers. Imagine having a database connection pool
/// that can be shared between different handlers and operation implementations.
/// State management can be expressed in a struct where the attributes hold the shared entities.
///
/// **NOTE: It is up to the implementation of the state structure to handle concurrency by protecting**
/// **its attributes using synchronization mechanisms.**
///
/// The framework stores the `Arc<T>` inside an [`http::Extensions`] and conveniently passes it to
/// the operation's implementation, making it able to handle operations with two different async signatures:
/// * `FnOnce(InputType) -> Future<OutputType>`
/// * `FnOnce(InputType, Extension<Arc<T>>) -> Future<OutputType>`
///
/// Wrapping the service with a [`tower::Layer`] will allow to have operations' signatures with and without shared state:
///
/// ```compile_fail
/// use std::sync::Arc;
/// use aws_smithy_http_server::{AddExtensionLayer, Extension, Router};
/// use tower::ServiceBuilder;
/// use tokio::sync::RwLock;
///
/// // Shared state,
/// #[derive(Debug, State)]
/// pub struct State {
///     pub count: RwLock<u64>
/// }
///
/// // Operation implementation with shared state.
/// async fn operation_with_state(input: Input, state: Extension<Arc<State>>) -> Output {
///     let mut count = state.0.write().await;
///     *count += 1;
///     Ok(Output::new())
/// }
///
/// // Operation implementation without shared state.
/// async fn operation_without_state(input: Input) -> Output {
///     Ok(Output::new())
/// }
///
/// let app: Router = OperationRegistryBuilder::default()
///     .operation_with_state(operation_with_state)
///     .operation_without_state(operation_without_state)
///     .build()
///     .unwrap()
///     .into();
/// let shared_state = Arc::new(State::default());
/// let app = app.layer(ServiceBuilder::new().layer(AddExtensionLayer::new(shared_state)));
/// let server =
///     axum::Server::bind(&"0.0.0.0:13734".parse().unwrap()).serve(app.into_make_service());
/// ...
/// ```
///
/// Without the middleware layer, the framework will require operations' signatures without
/// the shared state.
///
/// [`middleware`]: [`aws_smithy_http_server::AddExtensionLayer`]
#[derive(Debug, Default)]
pub struct State {
    calls_count: RwLock<u64>,
}

// Did you know your documentation will get rendered to build/brazil-documentation?
// You can find the docs by clicking the "docs" link on the "releases" tab in code.amazon.com.
//
// For packages released to live, the docs are also available at:
//
//   https://devcentral.amazon.com/ac/brazil/package-master/package/go/documentation?name=SmithyRsServerSdkPokemonService&interface=0.1&versionSet=live
//
// There are lots of good tips on writing good documentation at
//
//   https://doc.rust-lang.org/book/ch14-02-publishing-to-crates-io.html#making-useful-documentation-comments
//
// You can also add links to types, functions, and the like with [`hello`].
// This is a key part of making your documentation easier to navigate. See
//
//   https://doc.rust-lang.org/nightly/rustdoc/linking-to-items-by-name.html
//
// for more advanced linking syntax.

/// Retrieves information about a Pokémon species.
pub async fn get_pokemon_species(
    input: input::GetPokemonSpeciesInput,
    state: Extension<Arc<State>>,
) -> Result<output::GetPokemonSpeciesOutput, error::GetPokemonSpeciesError> {
    // We only support retrieving information about Pikachu.
    if input.name != "pikachu" {
        error!("Requested Pokémon {:?} not available", input.name);
        return Err(error::GetPokemonSpeciesError::ResourceNotFoundException(
            error::ResourceNotFoundException::builder()
                .message("Requested Pokémon not available")
                .build(),
        ));
    }
    // Increase the calls count by 1.
    let mut c = state.0.calls_count.write().await;
    *c += 1;

    info!("Requested Pokémon is pikacu, counter is {}", c);
    let english_flavor_text = model::FlavorText::builder()
        .flavor_text(PIKACHU_ENGLISH_FLAVOR_TEXT)
        .language("en")
        .build();
    let spanish_flavor_text = model::FlavorText::builder()
        .flavor_text(PIKACHU_SPANISH_FLAVOR_TEXT)
        .language("es")
        .build();
    let italian_flavor_text = model::FlavorText::builder()
        .flavor_text(PIKACHU_ITALIAN_FLAVOR_TEXT)
        .language("it")
        .build();
    let output = output::GetPokemonSpeciesOutput::builder()
        .name("pikachu")
        .flavor_text_entries(english_flavor_text)
        .flavor_text_entries(spanish_flavor_text)
        .flavor_text_entries(italian_flavor_text)
        .build();

    Ok(output)
}

/// Calculates and reports metrics about this server instance.
pub async fn get_server_statistics(
    _input: input::GetServerStatisticsInput,
    state: Extension<Arc<State>>,
) -> output::GetServerStatisticsOutput {
    // Read the current calls count.
    let counter = state.0.calls_count.read().await;
    let counter = (*counter)
        .try_into()
        .map_err(|e| {
            error!("Unable to convert u64 to i64: {}", e);
        })
        .unwrap_or(0);
    output::GetServerStatisticsOutput::builder()
        .calls_count(counter)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_pokemon_species_pikachu_spanish_flavor_text() {
        let input = input::GetPokemonSpeciesInput::builder()
            .name("pikachu")
            .build()
            .unwrap();

        let state = Arc::new(State::default());

        let actual_spanish_flavor_text = get_pokemon_species(input, Extension(state.clone()))
            .await
            .unwrap()
            .flavor_text_entries
            .unwrap()
            .into_iter()
            .find(|flavor_text| flavor_text.language == Some(String::from("es")))
            .unwrap();

        assert_eq!(
            PIKACHU_SPANISH_FLAVOR_TEXT,
            actual_spanish_flavor_text.flavor_text().unwrap()
        );

        let input = input::GetServerStatisticsInput::builder().build().unwrap();
        let stats = get_server_statistics(input, Extension(state.clone())).await;
        assert_eq!(1, stats.calls_count.unwrap());
    }
}
