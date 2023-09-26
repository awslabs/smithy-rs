/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::client::auth::no_auth::{NoAuthScheme, NO_AUTH_SCHEME_ID};
use crate::client::identity::no_auth::NoAuthIdentityResolver;
use crate::client::orchestrator::endpoints::StaticUriEndpointResolver;
use crate::client::retries::strategy::{NeverRetryStrategy, StandardRetryStrategy};
use aws_smithy_async::rt::sleep::SharedAsyncSleep;
use aws_smithy_async::time::SharedTimeSource;
use aws_smithy_http::result::SdkError;
use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::auth::static_resolver::StaticAuthSchemeOptionResolver;
use aws_smithy_runtime_api::client::auth::{
    AuthSchemeOptionResolverParams, SharedAuthScheme, SharedAuthSchemeOptionResolver,
};
use aws_smithy_runtime_api::client::connectors::SharedHttpConnector;
use aws_smithy_runtime_api::client::endpoint::{EndpointResolverParams, SharedEndpointResolver};
use aws_smithy_runtime_api::client::identity::SharedIdentityResolver;
use aws_smithy_runtime_api::client::interceptors::context::{Error, Input, Output};
use aws_smithy_runtime_api::client::interceptors::SharedInterceptor;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, OrchestratorError};
use aws_smithy_runtime_api::client::retries::{RetryClassifiers, SharedRetryStrategy};
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponentsBuilder;
use aws_smithy_runtime_api::client::runtime_plugin::{
    RuntimePlugins, SharedRuntimePlugin, StaticRuntimePlugin,
};
use aws_smithy_runtime_api::client::ser_de::{
    RequestSerializer, ResponseDeserializer, SharedRequestSerializer, SharedResponseDeserializer,
};
use aws_smithy_types::config_bag::{ConfigBag, Layer};
use aws_smithy_types::retry::RetryConfig;
use http::Uri;
use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;

struct FnSerializer<F, I> {
    f: F,
    _phantom: PhantomData<I>,
}
impl<F, I> FnSerializer<F, I> {
    fn new(f: F) -> Self {
        Self {
            f,
            _phantom: Default::default(),
        }
    }
}
impl<F, I> RequestSerializer for FnSerializer<F, I>
where
    F: Fn(I) -> Result<HttpRequest, BoxError> + Send + Sync,
    I: fmt::Debug + Send + Sync + 'static,
{
    fn serialize_input(&self, input: Input, _cfg: &mut ConfigBag) -> Result<HttpRequest, BoxError> {
        let input: I = input.downcast().expect("correct type");
        (self.f)(input)
    }
}
impl<F, I> fmt::Debug for FnSerializer<F, I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FnSerializer")
    }
}

struct FnDeserializer<F, O, E> {
    f: F,
    _phantom: PhantomData<(O, E)>,
}
impl<F, O, E> FnDeserializer<F, O, E> {
    fn new(deserializer: F) -> Self {
        Self {
            f: deserializer,
            _phantom: Default::default(),
        }
    }
}
impl<F, O, E> ResponseDeserializer for FnDeserializer<F, O, E>
where
    F: Fn(&HttpResponse) -> Result<O, OrchestratorError<E>> + Send + Sync,
    O: fmt::Debug + Send + Sync + 'static,
    E: std::error::Error + fmt::Debug + Send + Sync + 'static,
{
    fn deserialize_nonstreaming(
        &self,
        response: &HttpResponse,
    ) -> Result<Output, OrchestratorError<Error>> {
        (self.f)(response)
            .map(|output| Output::erase(output))
            .map_err(|err| err.map_operation_error(Error::erase))
    }
}
impl<F, O, E> fmt::Debug for FnDeserializer<F, O, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FnDeserializer")
    }
}

/// Orchestrates execution of a HTTP request without any modeled input or output.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct Operation<I, O, E> {
    service_name: Cow<'static, str>,
    operation_name: Cow<'static, str>,
    runtime_plugins: RuntimePlugins,
    _phantom: PhantomData<(I, O, E)>,
}

impl Operation<(), (), ()> {
    pub fn builder() -> OperationBuilder {
        OperationBuilder::new()
    }
}

impl<I, O, E> Operation<I, O, E>
where
    I: fmt::Debug + Send + Sync + 'static,
    O: fmt::Debug + Send + Sync + 'static,
    E: std::error::Error + fmt::Debug + Send + Sync + 'static,
{
    pub async fn invoke(&self, input: I) -> Result<O, SdkError<E, HttpResponse>> {
        let input = Input::erase(input);

        let output = super::invoke(
            &self.service_name,
            &self.operation_name,
            input,
            &self.runtime_plugins,
        )
        .await
        .map_err(|err| err.map_service_error(|e| e.downcast().expect("correct type")))?;

        Ok(output.downcast().expect("correct type"))
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct OperationBuilder<I = (), O = (), E = ()> {
    service_name: Option<Cow<'static, str>>,
    operation_name: Option<Cow<'static, str>>,
    config: Layer,
    runtime_components: RuntimeComponentsBuilder,
    runtime_plugins: Vec<SharedRuntimePlugin>,
    _phantom: PhantomData<(I, O, E)>,
}

impl Default for OperationBuilder<(), (), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationBuilder<(), (), ()> {
    pub fn new() -> Self {
        Self {
            service_name: None,
            operation_name: None,
            config: Layer::new("operation"),
            runtime_components: RuntimeComponentsBuilder::new("operation"),
            runtime_plugins: Vec::new(),
            _phantom: Default::default(),
        }
    }
}

impl<I, O, E> OperationBuilder<I, O, E> {
    pub fn service_name(mut self, service_name: impl Into<Cow<'static, str>>) -> Self {
        self.service_name = Some(service_name.into());
        self
    }

    pub fn operation_name(mut self, operation_name: impl Into<Cow<'static, str>>) -> Self {
        self.operation_name = Some(operation_name.into());
        self
    }

    pub fn http_connector(mut self, connector: SharedHttpConnector) -> Self {
        self.runtime_components.set_http_connector(Some(connector));
        self
    }

    pub fn endpoint_url(mut self, url: &str) -> Self {
        self.config.store_put(EndpointResolverParams::new(()));
        self.runtime_components
            .set_endpoint_resolver(Some(SharedEndpointResolver::new(
                StaticUriEndpointResolver::uri(Uri::try_from(url).expect("valid URI")),
            )));
        self
    }

    pub fn no_retry(mut self) -> Self {
        self.runtime_components
            .set_retry_strategy(Some(SharedRetryStrategy::new(NeverRetryStrategy::new())));
        self
    }

    pub fn retry_classifiers(mut self, retry_classifiers: RetryClassifiers) -> Self {
        self.runtime_components
            .set_retry_classifiers(Some(retry_classifiers));
        self
    }

    pub fn standard_retry(mut self, retry_config: &RetryConfig) -> Self {
        self.runtime_components
            .set_retry_strategy(Some(SharedRetryStrategy::new(StandardRetryStrategy::new(
                retry_config,
            ))));
        self
    }

    pub fn no_auth(mut self) -> Self {
        self.config
            .store_put(AuthSchemeOptionResolverParams::new(()));
        self.runtime_components
            .set_auth_scheme_option_resolver(Some(SharedAuthSchemeOptionResolver::new(
                StaticAuthSchemeOptionResolver::new(vec![NO_AUTH_SCHEME_ID]),
            )));
        self.runtime_components
            .push_auth_scheme(SharedAuthScheme::new(NoAuthScheme::default()));
        self.runtime_components.push_identity_resolver(
            NO_AUTH_SCHEME_ID,
            SharedIdentityResolver::new(NoAuthIdentityResolver::new()),
        );
        self
    }

    pub fn sleep_impl(mut self, async_sleep: SharedAsyncSleep) -> Self {
        self.runtime_components.set_sleep_impl(Some(async_sleep));
        self
    }

    pub fn time_source(mut self, time_source: SharedTimeSource) -> Self {
        self.runtime_components.set_time_source(Some(time_source));
        self
    }

    pub fn interceptor(mut self, interceptor: SharedInterceptor) -> Self {
        self.runtime_components.push_interceptor(interceptor);
        self
    }

    pub fn runtime_plugin(mut self, runtime_plugin: SharedRuntimePlugin) -> Self {
        self.runtime_plugins.push(runtime_plugin);
        self
    }

    pub fn serializer<I2>(
        mut self,
        serializer: impl Fn(I2) -> Result<HttpRequest, BoxError> + Send + Sync + 'static,
    ) -> OperationBuilder<I2, O, E>
    where
        I2: fmt::Debug + Send + Sync + 'static,
    {
        self.config
            .store_put(SharedRequestSerializer::new(FnSerializer::new(serializer)));
        OperationBuilder {
            service_name: self.service_name,
            operation_name: self.operation_name,
            config: self.config,
            runtime_components: self.runtime_components,
            runtime_plugins: self.runtime_plugins,
            _phantom: Default::default(),
        }
    }

    pub fn deserializer<O2, E2>(
        mut self,
        deserializer: impl Fn(&HttpResponse) -> Result<O2, OrchestratorError<E2>>
            + Send
            + Sync
            + 'static,
    ) -> OperationBuilder<I, O2, E2>
    where
        O2: fmt::Debug + Send + Sync + 'static,
        E2: std::error::Error + fmt::Debug + Send + Sync + 'static,
    {
        self.config
            .store_put(SharedResponseDeserializer::new(FnDeserializer::new(
                deserializer,
            )));
        OperationBuilder {
            service_name: self.service_name,
            operation_name: self.operation_name,
            config: self.config,
            runtime_components: self.runtime_components,
            runtime_plugins: self.runtime_plugins,
            _phantom: Default::default(),
        }
    }

    pub fn build(self) -> Operation<I, O, E> {
        let service_name = self.service_name.expect("service_name required");
        let operation_name = self.operation_name.expect("operation_name required");
        assert!(
            self.runtime_components.http_connector().is_some(),
            "a http_connector is required"
        );
        assert!(
            self.runtime_components.endpoint_resolver().is_some(),
            "a endpoint_resolver is required"
        );
        assert!(
            self.runtime_components.retry_strategy().is_some(),
            "a retry_strategy is required"
        );
        assert!(
            self.config.load::<SharedRequestSerializer>().is_some(),
            "a serializer is required"
        );
        assert!(
            self.config.load::<SharedResponseDeserializer>().is_some(),
            "a deserializer is required"
        );
        let mut runtime_plugins = RuntimePlugins::new().with_client_plugin(
            StaticRuntimePlugin::new()
                .with_config(self.config.freeze())
                .with_runtime_components(self.runtime_components),
        );
        for runtime_plugin in self.runtime_plugins {
            runtime_plugins = runtime_plugins.with_client_plugin(runtime_plugin);
        }

        Operation {
            service_name,
            operation_name,
            runtime_plugins,
            _phantom: Default::default(),
        }
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    use super::*;
    use crate::client::connectors::test_util::{capture_request, ConnectionEvent, EventConnector};
    use crate::client::retries::classifier::HttpStatusCodeClassifier;
    use aws_smithy_async::rt::sleep::{SharedAsyncSleep, TokioSleep};
    use aws_smithy_http::body::SdkBody;
    use aws_smithy_http::result::ConnectorError;
    use std::convert::Infallible;

    #[tokio::test]
    async fn operation() {
        let (connector, request_rx) = capture_request(Some(
            http::Response::builder()
                .status(418)
                .body(SdkBody::from(&b"I'm a teapot!"[..]))
                .unwrap(),
        ));
        let operation = Operation::builder()
            .service_name("test")
            .operation_name("test")
            .http_connector(SharedHttpConnector::new(connector))
            .endpoint_url("http://localhost:1234")
            .no_auth()
            .no_retry()
            .serializer(|input: String| {
                Ok(http::Request::builder()
                    .body(SdkBody::from(input.as_bytes()))
                    .unwrap())
            })
            .deserializer::<_, Infallible>(|response| {
                assert_eq!(418, response.status());
                Ok(std::str::from_utf8(response.body().bytes().unwrap())
                    .unwrap()
                    .to_string())
            })
            .build();

        let output = operation
            .invoke("what are you?".to_string())
            .await
            .expect("success");
        assert_eq!("I'm a teapot!", output);

        let request = request_rx.expect_request();
        assert_eq!("http://localhost:1234", request.uri());
        assert_eq!(b"what are you?", request.body().bytes().unwrap());
    }

    #[tokio::test]
    async fn operation_retries() {
        let connector = EventConnector::new(
            vec![
                ConnectionEvent::new(
                    http::Request::builder()
                        .uri("http://localhost:1234/")
                        .body(SdkBody::from(&b"what are you?"[..]))
                        .unwrap(),
                    http::Response::builder()
                        .status(503)
                        .body(SdkBody::from(&b""[..]))
                        .unwrap(),
                ),
                ConnectionEvent::new(
                    http::Request::builder()
                        .uri("http://localhost:1234/")
                        .body(SdkBody::from(&b"what are you?"[..]))
                        .unwrap(),
                    http::Response::builder()
                        .status(418)
                        .body(SdkBody::from(&b"I'm a teapot!"[..]))
                        .unwrap(),
                ),
            ],
            SharedAsyncSleep::new(TokioSleep::new()),
        );
        let operation = Operation::builder()
            .service_name("test")
            .operation_name("test")
            .http_connector(SharedHttpConnector::new(connector.clone()))
            .endpoint_url("http://localhost:1234")
            .no_auth()
            .retry_classifiers(
                RetryClassifiers::new().with_classifier(HttpStatusCodeClassifier::default()),
            )
            .standard_retry(&RetryConfig::standard())
            .sleep_impl(SharedAsyncSleep::new(TokioSleep::new()))
            .serializer(|input: String| {
                Ok(http::Request::builder()
                    .body(SdkBody::from(input.as_bytes()))
                    .unwrap())
            })
            .deserializer::<_, Infallible>(|response| {
                if response.status() == 503 {
                    Err(OrchestratorError::connector(ConnectorError::io(
                        "test".into(),
                    )))
                } else {
                    assert_eq!(418, response.status());
                    Ok(std::str::from_utf8(response.body().bytes().unwrap())
                        .unwrap()
                        .to_string())
                }
            })
            .build();

        let output = operation
            .invoke("what are you?".to_string())
            .await
            .expect("success");
        assert_eq!("I'm a teapot!", output);

        connector.assert_requests_match(&[]);
    }
}
