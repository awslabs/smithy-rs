/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.generators.protocol

import software.amazon.smithy.model.shapes.BlobShape
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.ClientRustModule
import software.amazon.smithy.rust.codegen.client.smithy.generators.http.RequestBindingGenerator
import software.amazon.smithy.rust.codegen.client.smithy.protocols.ClientAdditionalPayloadContext
import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.generators.protocol.ProtocolPayloadGenerator
import software.amazon.smithy.rust.codegen.core.smithy.protocols.HttpLocation
import software.amazon.smithy.rust.codegen.core.smithy.protocols.Protocol
import software.amazon.smithy.rust.codegen.core.util.dq
import software.amazon.smithy.rust.codegen.core.util.findStreamingMember
import software.amazon.smithy.rust.codegen.core.util.inputShape

class RequestSerializerGenerator(
    private val codegenContext: ClientCodegenContext,
    private val protocol: Protocol,
    private val bodyGenerator: ProtocolPayloadGenerator,
) {
    private val httpBindingResolver = protocol.httpBindingResolver
    private val symbolProvider = codegenContext.symbolProvider
    private val codegenScope by lazy {
        val runtimeApi = CargoDependency.smithyRuntimeApi(codegenContext.runtimeConfig).toType()
        val interceptorContext = runtimeApi.resolve("client::interceptors::context")
        val orchestrator = runtimeApi.resolve("client::orchestrator")
        val smithyTypes = CargoDependency.smithyTypes(codegenContext.runtimeConfig).toType()
        arrayOf(
            "BoxError" to orchestrator.resolve("BoxError"),
            "ConfigBag" to smithyTypes.resolve("config_bag::ConfigBag"),
            "HttpRequest" to orchestrator.resolve("HttpRequest"),
            "HttpRequestBuilder" to RuntimeType.HttpRequestBuilder,
            "Input" to interceptorContext.resolve("Input"),
            "RequestSerializer" to orchestrator.resolve("RequestSerializer"),
            "SdkBody" to RuntimeType.sdkBody(codegenContext.runtimeConfig),
            "TypedBox" to smithyTypes.resolve("type_erasure::TypedBox"),
            "config" to ClientRustModule.Config,
            "header_util" to RuntimeType.smithyHttp(codegenContext.runtimeConfig).resolve("header"),
            "http" to RuntimeType.Http,
            "operation" to RuntimeType.operationModule(codegenContext.runtimeConfig),
        )
    }

    fun render(writer: RustWriter, operationShape: OperationShape) {
        val inputShape = operationShape.inputShape(codegenContext.model)
        val operationName = symbolProvider.toSymbol(operationShape).name
        val inputSymbol = symbolProvider.toSymbol(inputShape)
        writer.rustTemplate(
            """
            ##[derive(Debug)]
            struct ${operationName}RequestSerializer;
            impl #{RequestSerializer} for ${operationName}RequestSerializer {
                ##[allow(unused_mut, clippy::let_and_return, clippy::needless_borrow, clippy::useless_conversion)]
                fn serialize_input(&self, input: #{Input}, _cfg: &mut #{ConfigBag}) -> Result<#{HttpRequest}, #{BoxError}> {
                    let input = #{TypedBox}::<#{ConcreteInput}>::assume_from(input).expect("correct type").unwrap();
                    let mut request_builder = {
                        #{create_http_request}
                    };
                    let body = #{generate_body};
                    #{add_content_length}
                    Ok(request_builder.body(body).expect("valid request"))
                }
            }
            """,
            *codegenScope,
            "ConcreteInput" to inputSymbol,
            "create_http_request" to createHttpRequest(operationShape),
            "generate_body" to writable {
                val body = writable {
                    bodyGenerator.generatePayload(
                        this,
                        "input",
                        operationShape,
                        ClientAdditionalPayloadContext(propertyBagAvailable = false),
                    )
                }
                val streamingMember = inputShape.findStreamingMember(codegenContext.model)
                val isBlobStreaming =
                    streamingMember != null && codegenContext.model.expectShape(streamingMember.target) is BlobShape
                if (isBlobStreaming) {
                    // Consume the `ByteStream` into its inner `SdkBody`.
                    rust("#T.into_inner()", body)
                } else {
                    rustTemplate("#{SdkBody}::from(#{body})", *codegenScope, "body" to body)
                }
            },
            "add_content_length" to if (needsContentLength(operationShape)) {
                writable {
                    rustTemplate(
                        """
                        if let Some(content_length) = body.content_length() {
                            request_builder = #{header_util}::set_request_header_if_absent(request_builder, #{http}::header::CONTENT_LENGTH, content_length);
                        }
                        """,
                        *codegenScope,
                    )
                }
            } else {
                writable { }
            },
        )
    }

    private fun needsContentLength(operationShape: OperationShape): Boolean {
        return protocol.httpBindingResolver.requestBindings(operationShape)
            .any { it.location == HttpLocation.DOCUMENT || it.location == HttpLocation.PAYLOAD }
    }

    private fun createHttpRequest(operationShape: OperationShape): Writable = writable {
        val httpBindingGenerator = RequestBindingGenerator(
            codegenContext,
            protocol,
            operationShape,
        )
        httpBindingGenerator.renderUpdateHttpBuilder(this)
        val contentType = httpBindingResolver.requestContentType(operationShape)

        rust("let mut builder = update_http_builder(&input, #T::new())?;", RuntimeType.HttpRequestBuilder)
        if (contentType != null) {
            rustTemplate(
                "builder = #{header_util}::set_request_header_if_absent(builder, #{http}::header::CONTENT_TYPE, ${contentType.dq()});",
                *codegenScope,
            )
        }
        for (header in protocol.additionalRequestHeaders(operationShape)) {
            rustTemplate(
                """
                builder = #{header_util}::set_request_header_if_absent(
                    builder,
                    #{http}::header::HeaderName::from_static(${header.first.dq()}),
                    ${header.second.dq()}
                );
                """,
                *codegenScope,
            )
        }
        rust("builder")
    }
}
