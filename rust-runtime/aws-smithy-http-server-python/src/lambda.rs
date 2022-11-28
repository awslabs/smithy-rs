/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Python wrappers for Lambda related types.

use std::collections::HashMap;

use lambda_http::Context;
use pyo3::pyclass;

/// AWS Mobile SDK client fields.
#[pyclass]
#[derive(Clone)]
struct PyClientApplication {
    /// The mobile app installation id
    #[pyo3(get)]
    installation_id: String,
    /// The app title for the mobile app as registered with AWS' mobile services.
    #[pyo3(get)]
    app_title: String,
    /// The version name of the application as registered with AWS' mobile services.
    #[pyo3(get)]
    app_version_name: String,
    /// The app version code.
    #[pyo3(get)]
    app_version_code: String,
    /// The package name for the mobile application invoking the function
    #[pyo3(get)]
    app_package_name: String,
}

/// Client context sent by the AWS Mobile SDK.
#[pyclass]
#[derive(Clone)]
struct PyClientContext {
    /// Information about the mobile application invoking the function.
    #[pyo3(get)]
    client: PyClientApplication,
    /// Custom properties attached to the mobile event context.
    #[pyo3(get)]
    custom: HashMap<String, String>,
    /// Environment settings from the mobile client.
    #[pyo3(get)]
    environment: HashMap<String, String>,
}

/// Cognito identity information sent with the event
#[pyclass]
#[derive(Clone)]
struct PyCognitoIdentity {
    /// The unique identity id for the Cognito credentials invoking the function.
    #[pyo3(get)]
    identity_id: String,
    /// The identity pool id the caller is "registered" with.
    #[pyo3(get)]
    identity_pool_id: String,
}

/// Configuration derived from environment variables.
#[pyclass]
#[derive(Clone)]
struct PyConfig {
    /// The name of the function.
    #[pyo3(get)]
    function_name: String,
    /// The amount of memory available to the function in MB.
    #[pyo3(get)]
    memory: i32,
    /// The version of the function being executed.
    #[pyo3(get)]
    version: String,
    /// The name of the Amazon CloudWatch Logs stream for the function.
    #[pyo3(get)]
    log_stream: String,
    /// The name of the Amazon CloudWatch Logs group for the function.
    #[pyo3(get)]
    log_group: String,
}

/// The Lambda function execution context. The values in this struct
/// are populated using the [Lambda environment variables](https://docs.aws.amazon.com/lambda/latest/dg/current-supported-versions.html)
/// and the headers returned by the poll request to the Runtime APIs.
#[derive(Clone)]
#[pyclass(name = "LambdaContext")]
pub struct PyLambdaContext {
    /// The AWS request ID generated by the Lambda service.
    #[pyo3(get)]
    request_id: String,
    /// The execution deadline for the current invocation in milliseconds.
    #[pyo3(get)]
    deadline: u64,
    /// The ARN of the Lambda function being invoked.
    #[pyo3(get)]
    invoked_function_arn: String,
    /// The X-Ray trace ID for the current invocation.
    #[pyo3(get)]
    xray_trace_id: String,
    /// The client context object sent by the AWS mobile SDK. This field is
    /// empty unless the function is invoked using an AWS mobile SDK.
    #[pyo3(get)]
    client_context: Option<PyClientContext>,
    /// The Cognito identity that invoked the function. This field is empty
    /// unless the invocation request to the Lambda APIs was made using AWS
    /// credentials issues by Amazon Cognito Identity Pools.
    #[pyo3(get)]
    identity: Option<PyCognitoIdentity>,
    /// Lambda function configuration from the local environment variables.
    /// Includes information such as the function name, memory allocation,
    /// version, and log streams.
    #[pyo3(get)]
    env_config: PyConfig,
}

impl PyLambdaContext {
    /// Create Python-compatible version of [Context].
    pub fn new(ctx: Context) -> Self {
        Self {
            request_id: ctx.request_id,
            deadline: ctx.deadline,
            invoked_function_arn: ctx.invoked_function_arn,
            xray_trace_id: ctx.xray_trace_id,
            client_context: ctx.client_context.map(|client_ctx| PyClientContext {
                client: PyClientApplication {
                    installation_id: client_ctx.client.installation_id,
                    app_title: client_ctx.client.app_title,
                    app_version_name: client_ctx.client.app_version_name,
                    app_version_code: client_ctx.client.app_version_code,
                    app_package_name: client_ctx.client.app_package_name,
                },
                custom: client_ctx.custom,
                environment: client_ctx.environment,
            }),
            identity: ctx.identity.map(|identity| PyCognitoIdentity {
                identity_id: identity.identity_id,
                identity_pool_id: identity.identity_pool_id,
            }),
            env_config: PyConfig {
                function_name: ctx.env_config.function_name,
                memory: ctx.env_config.memory,
                version: ctx.env_config.version,
                log_stream: ctx.env_config.log_stream,
                log_group: ctx.env_config.log_group,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use http::{header::HeaderName, HeaderMap, HeaderValue};
    use lambda_http::lambda_runtime::{Config, Context};
    use pyo3::{prelude::*, py_run};

    use super::*;

    #[test]
    fn py_lambda_context() -> PyResult<()> {
        pyo3::prepare_freethreaded_python();

        let headers = HeaderMap::from_iter([
            (
                HeaderName::from_static("lambda-runtime-aws-request-id"),
                HeaderValue::from_static("my-id"),
            ),
            (
                HeaderName::from_static("lambda-runtime-deadline-ms"),
                HeaderValue::from_static("123"),
            ),
            (
                HeaderName::from_static("lambda-runtime-invoked-function-arn"),
                HeaderValue::from_static("arn::myarn"),
            ),
            (
                HeaderName::from_static("lambda-runtime-trace-id"),
                HeaderValue::from_static("my-trace-id"),
            ),
            (
                HeaderName::from_static("lambda-runtime-client-context"),
                HeaderValue::from_str(
                    &r#"
{
    "client": {
        "installationId": "my-installation-id",
        "appTitle": "my-app-title",
        "appVersionName": "my-app-version-name",
        "appVersionCode": "my-app-version-code",
        "appPackageName": "my-app-package-name"
    },
    "custom": {
        "custom-key": "custom-val"
    },
    "environment": {
        "environment-key": "environment-val"
    }
}
"#
                    .split_whitespace()
                    .collect::<String>(),
                )
                .unwrap(),
            ),
            (
                HeaderName::from_static("lambda-runtime-cognito-identity"),
                HeaderValue::from_str(
                    &r#"
{
    "identity_id": "my-identity-id",
    "identity_pool_id": "my-identity-pool-id"
}
"#
                    .split_whitespace()
                    .collect::<String>(),
                )
                .unwrap(),
            ),
        ]);
        let lambda_context = Context::try_from(headers).unwrap();
        let lambda_context = lambda_context.with_config(&Config {
            function_name: "my-fn".to_string(),
            memory: 128,
            version: "my-version".to_string(),
            log_stream: "my-log-stream".to_string(),
            log_group: "my-log-group".to_string(),
        });

        Python::with_gil(|py| {
            let ctx = PyCell::new(py, PyLambdaContext::new(lambda_context))?;
            py_run!(
                py,
                ctx,
                r#"
assert ctx.request_id == "my-id"
assert ctx.deadline == 123
assert ctx.invoked_function_arn == "arn::myarn"
assert ctx.xray_trace_id == "my-trace-id"

assert ctx.client_context.client.installation_id == "my-installation-id"
assert ctx.client_context.client.app_title == "my-app-title"
assert ctx.client_context.client.app_version_name == "my-app-version-name"
assert ctx.client_context.client.app_version_code == "my-app-version-code"
assert ctx.client_context.client.app_package_name == "my-app-package-name"
assert ctx.client_context.custom == {"custom-key":"custom-val"}
assert ctx.client_context.environment == {"environment-key":"environment-val"}

assert ctx.identity.identity_id == "my-identity-id"
assert ctx.identity.identity_pool_id == "my-identity-pool-id"

assert ctx.env_config.function_name == "my-fn"
assert ctx.env_config.memory == 128
assert ctx.env_config.version == "my-version"
assert ctx.env_config.log_stream == "my-log-stream"
assert ctx.env_config.log_group == "my-log-group"
"#
            );
            Ok(())
        })
    }
}
