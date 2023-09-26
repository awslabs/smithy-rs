/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.customizations

import org.junit.jupiter.api.Test
import software.amazon.smithy.rust.codegen.client.testutil.clientIntegrationTest
import software.amazon.smithy.rust.codegen.core.rustlang.Attribute
import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.testutil.asSmithyModel
import software.amazon.smithy.rust.codegen.core.testutil.integrationTest

class HttpAuthDecoratorTest {
    private fun codegenScope(runtimeConfig: RuntimeConfig): Array<Pair<String, Any>> = arrayOf(
        "ConnectionEvent" to CargoDependency.smithyRuntime(runtimeConfig)
            .toDevDependency().withFeature("test-util").toType()
            .resolve("client::http::test_util::ConnectionEvent"),
        "EventClient" to CargoDependency.smithyRuntime(runtimeConfig)
            .toDevDependency().withFeature("test-util").toType()
            .resolve("client::http::test_util::EventClient"),
        "SdkBody" to RuntimeType.sdkBody(runtimeConfig),
        "TokioSleep" to CargoDependency.smithyAsync(runtimeConfig).withFeature("rt-tokio").toType()
            .resolve("rt::sleep::TokioSleep"),
    )

    @Test
    fun multipleAuthSchemesSchemeSelection() {
        clientIntegrationTest(TestModels.allSchemes) { codegenContext, rustCrate ->
            rustCrate.integrationTest("tests") {
                val moduleName = codegenContext.moduleUseName()
                Attribute.TokioTest.render(this)
                rustTemplate(
                    """
                    async fn use_api_key_auth_when_api_key_provided() {
                        use aws_smithy_runtime_api::client::identity::http::Token;

                        let http_client = #{EventClient}::new(
                            vec![#{ConnectionEvent}::new(
                                http::Request::builder()
                                    .uri("http://localhost:1234/SomeOperation?api_key=some-api-key")
                                    .body(#{SdkBody}::empty())
                                    .unwrap(),
                                http::Response::builder().status(200).body(#{SdkBody}::empty()).unwrap(),
                            )],
                            #{TokioSleep}::new(),
                        );

                        let config = $moduleName::Config::builder()
                            .api_key(Token::new("some-api-key", None))
                            .endpoint_resolver("http://localhost:1234")
                            .http_client(http_client.clone())
                            .build();
                        let client = $moduleName::Client::from_conf(config);
                        let _ = client.some_operation()
                            .send()
                            .await
                            .expect("success");
                        http_client.assert_requests_match(&[]);
                    }
                    """,
                    *codegenScope(codegenContext.runtimeConfig),
                )
                Attribute.TokioTest.render(this)
                rustTemplate(
                    """
                    async fn use_basic_auth_when_basic_auth_login_provided() {
                        use aws_smithy_runtime_api::client::identity::http::Login;

                        let http_client = #{EventClient}::new(
                            vec![#{ConnectionEvent}::new(
                                http::Request::builder()
                                    .header("authorization", "Basic c29tZS11c2VyOnNvbWUtcGFzcw==")
                                    .uri("http://localhost:1234/SomeOperation")
                                    .body(#{SdkBody}::empty())
                                    .unwrap(),
                                http::Response::builder().status(200).body(#{SdkBody}::empty()).unwrap(),
                            )],
                            #{TokioSleep}::new(),
                        );

                        let config = $moduleName::Config::builder()
                            .basic_auth_login(Login::new("some-user", "some-pass", None))
                            .endpoint_resolver("http://localhost:1234")
                            .http_client(http_client.clone())
                            .build();
                        let client = $moduleName::Client::from_conf(config);
                        let _ = client.some_operation()
                            .send()
                            .await
                            .expect("success");
                        http_client.assert_requests_match(&[]);
                    }
                    """,
                    *codegenScope(codegenContext.runtimeConfig),
                )
            }
        }
    }

    @Test
    fun apiKeyInQueryString() {
        clientIntegrationTest(TestModels.apiKeyInQueryString) { codegenContext, rustCrate ->
            rustCrate.integrationTest("api_key_applied_to_query_string") {
                val moduleName = codegenContext.moduleUseName()
                Attribute.TokioTest.render(this)
                rustTemplate(
                    """
                    async fn api_key_applied_to_query_string() {
                        use aws_smithy_runtime_api::client::identity::http::Token;

                        let http_client = #{EventClient}::new(
                            vec![#{ConnectionEvent}::new(
                                http::Request::builder()
                                    .uri("http://localhost:1234/SomeOperation?api_key=some-api-key")
                                    .body(#{SdkBody}::empty())
                                    .unwrap(),
                                http::Response::builder().status(200).body(#{SdkBody}::empty()).unwrap(),
                            )],
                            #{TokioSleep}::new(),
                        );

                        let config = $moduleName::Config::builder()
                            .api_key(Token::new("some-api-key", None))
                            .endpoint_resolver("http://localhost:1234")
                            .http_client(http_client.clone())
                            .build();
                        let client = $moduleName::Client::from_conf(config);
                        let _ = client.some_operation()
                            .send()
                            .await
                            .expect("success");
                        http_client.assert_requests_match(&[]);
                    }
                    """,
                    *codegenScope(codegenContext.runtimeConfig),
                )
            }
        }
    }

    @Test
    fun apiKeyInHeaders() {
        clientIntegrationTest(TestModels.apiKeyInHeaders) { codegenContext, rustCrate ->
            rustCrate.integrationTest("api_key_applied_to_headers") {
                val moduleName = codegenContext.moduleUseName()
                Attribute.TokioTest.render(this)
                rustTemplate(
                    """
                    async fn api_key_applied_to_headers() {
                        use aws_smithy_runtime_api::client::identity::http::Token;

                        let http_client = #{EventClient}::new(
                            vec![#{ConnectionEvent}::new(
                                http::Request::builder()
                                    .header("authorization", "ApiKey some-api-key")
                                    .uri("http://localhost:1234/SomeOperation")
                                    .body(#{SdkBody}::empty())
                                    .unwrap(),
                                http::Response::builder().status(200).body(#{SdkBody}::empty()).unwrap(),
                            )],
                            #{TokioSleep}::new(),
                        );

                        let config = $moduleName::Config::builder()
                            .api_key(Token::new("some-api-key", None))
                            .endpoint_resolver("http://localhost:1234")
                            .http_client(http_client.clone())
                            .build();
                        let client = $moduleName::Client::from_conf(config);
                        let _ = client.some_operation()
                            .send()
                            .await
                            .expect("success");
                        http_client.assert_requests_match(&[]);
                    }
                    """,
                    *codegenScope(codegenContext.runtimeConfig),
                )
            }
        }
    }

    @Test
    fun basicAuth() {
        clientIntegrationTest(TestModels.basicAuth) { codegenContext, rustCrate ->
            rustCrate.integrationTest("basic_auth") {
                val moduleName = codegenContext.moduleUseName()
                Attribute.TokioTest.render(this)
                rustTemplate(
                    """
                    async fn basic_auth() {
                        use aws_smithy_runtime_api::client::identity::http::Login;

                        let http_client = #{EventClient}::new(
                            vec![#{ConnectionEvent}::new(
                                http::Request::builder()
                                    .header("authorization", "Basic c29tZS11c2VyOnNvbWUtcGFzcw==")
                                    .uri("http://localhost:1234/SomeOperation")
                                    .body(#{SdkBody}::empty())
                                    .unwrap(),
                                http::Response::builder().status(200).body(#{SdkBody}::empty()).unwrap(),
                            )],
                            #{TokioSleep}::new(),
                        );

                        let config = $moduleName::Config::builder()
                            .basic_auth_login(Login::new("some-user", "some-pass", None))
                            .endpoint_resolver("http://localhost:1234")
                            .http_client(http_client.clone())
                            .build();
                        let client = $moduleName::Client::from_conf(config);
                        let _ = client.some_operation()
                            .send()
                            .await
                            .expect("success");
                        http_client.assert_requests_match(&[]);
                    }
                    """,
                    *codegenScope(codegenContext.runtimeConfig),
                )
            }
        }
    }

    @Test
    fun bearerAuth() {
        clientIntegrationTest(TestModels.bearerAuth) { codegenContext, rustCrate ->
            rustCrate.integrationTest("bearer_auth") {
                val moduleName = codegenContext.moduleUseName()
                Attribute.TokioTest.render(this)
                rustTemplate(
                    """
                    async fn basic_auth() {
                        use aws_smithy_runtime_api::client::identity::http::Token;

                        let http_client = #{EventClient}::new(
                            vec![#{ConnectionEvent}::new(
                                http::Request::builder()
                                    .header("authorization", "Bearer some-token")
                                    .uri("http://localhost:1234/SomeOperation")
                                    .body(#{SdkBody}::empty())
                                    .unwrap(),
                                http::Response::builder().status(200).body(#{SdkBody}::empty()).unwrap(),
                            )],
                            #{TokioSleep}::new(),
                        );

                        let config = $moduleName::Config::builder()
                            .bearer_token(Token::new("some-token", None))
                            .endpoint_resolver("http://localhost:1234")
                            .http_client(http_client.clone())
                            .build();
                        let client = $moduleName::Client::from_conf(config);
                        let _ = client.some_operation()
                            .send()
                            .await
                            .expect("success");
                        http_client.assert_requests_match(&[]);
                    }
                    """,
                    *codegenScope(codegenContext.runtimeConfig),
                )
            }
        }
    }

    @Test
    fun optionalAuth() {
        clientIntegrationTest(TestModels.optionalAuth) { codegenContext, rustCrate ->
            rustCrate.integrationTest("optional_auth") {
                val moduleName = codegenContext.moduleUseName()
                Attribute.TokioTest.render(this)
                rustTemplate(
                    """
                    async fn optional_auth() {
                        let http_client = #{EventClient}::new(
                            vec![#{ConnectionEvent}::new(
                                http::Request::builder()
                                    .uri("http://localhost:1234/SomeOperation")
                                    .body(#{SdkBody}::empty())
                                    .unwrap(),
                                http::Response::builder().status(200).body(#{SdkBody}::empty()).unwrap(),
                            )],
                            #{TokioSleep}::new(),
                        );

                        let config = $moduleName::Config::builder()
                            .endpoint_resolver("http://localhost:1234")
                            .http_client(http_client.clone())
                            .build();
                        let client = $moduleName::Client::from_conf(config);
                        let _ = client.some_operation()
                            .send()
                            .await
                            .expect("success");
                        http_client.assert_requests_match(&[]);
                    }
                    """,
                    *codegenScope(codegenContext.runtimeConfig),
                )
            }
        }
    }
}

private object TestModels {
    val allSchemes = """
        namespace test

        use aws.api#service
        use aws.protocols#restJson1

        @service(sdkId: "Test Api Key Auth")
        @restJson1
        @httpApiKeyAuth(name: "api_key", in: "query")
        @httpBasicAuth
        @httpBearerAuth
        @httpDigestAuth
        @auth([httpApiKeyAuth, httpBasicAuth, httpBearerAuth, httpDigestAuth])
        service TestService {
            version: "2023-01-01",
            operations: [SomeOperation]
        }

        structure SomeOutput {
            someAttribute: Long,
            someVal: String
        }

        @http(uri: "/SomeOperation", method: "GET")
        operation SomeOperation {
            output: SomeOutput
        }
    """.asSmithyModel()

    val apiKeyInQueryString = """
        namespace test

        use aws.api#service
        use aws.protocols#restJson1

        @service(sdkId: "Test Api Key Auth")
        @restJson1
        @httpApiKeyAuth(name: "api_key", in: "query")
        @auth([httpApiKeyAuth])
        service TestService {
            version: "2023-01-01",
            operations: [SomeOperation]
        }

        structure SomeOutput {
            someAttribute: Long,
            someVal: String
        }

        @http(uri: "/SomeOperation", method: "GET")
        operation SomeOperation {
            output: SomeOutput
        }
    """.asSmithyModel()

    val apiKeyInHeaders = """
        namespace test

        use aws.api#service
        use aws.protocols#restJson1

        @service(sdkId: "Test Api Key Auth")
        @restJson1
        @httpApiKeyAuth(name: "authorization", in: "header", scheme: "ApiKey")
        @auth([httpApiKeyAuth])
        service TestService {
            version: "2023-01-01",
            operations: [SomeOperation]
        }

        structure SomeOutput {
            someAttribute: Long,
            someVal: String
        }

        @http(uri: "/SomeOperation", method: "GET")
        operation SomeOperation {
            output: SomeOutput
        }
    """.asSmithyModel()

    val basicAuth = """
        namespace test

        use aws.api#service
        use aws.protocols#restJson1

        @service(sdkId: "Test Api Key Auth")
        @restJson1
        @httpBasicAuth
        @auth([httpBasicAuth])
        service TestService {
            version: "2023-01-01",
            operations: [SomeOperation]
        }

        structure SomeOutput {
            someAttribute: Long,
            someVal: String
        }

        @http(uri: "/SomeOperation", method: "GET")
        operation SomeOperation {
            output: SomeOutput
        }
    """.asSmithyModel()

    val bearerAuth = """
        namespace test

        use aws.api#service
        use aws.protocols#restJson1

        @service(sdkId: "Test Api Key Auth")
        @restJson1
        @httpBearerAuth
        @auth([httpBearerAuth])
        service TestService {
            version: "2023-01-01",
            operations: [SomeOperation]
        }

        structure SomeOutput {
            someAttribute: Long,
            someVal: String
        }

        @http(uri: "/SomeOperation", method: "GET")
        operation SomeOperation {
            output: SomeOutput
        }
    """.asSmithyModel()

    val optionalAuth = """
        namespace test

        use aws.api#service
        use aws.protocols#restJson1

        @service(sdkId: "Test Api Key Auth")
        @restJson1
        @httpBearerAuth
        @auth([httpBearerAuth])
        service TestService {
            version: "2023-01-01",
            operations: [SomeOperation]
        }

        structure SomeOutput {
            someAttribute: Long,
            someVal: String
        }

        @http(uri: "/SomeOperation", method: "GET")
        @optionalAuth
        operation SomeOperation {
            output: SomeOutput
        }
    """.asSmithyModel()
}
