/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.protocols.parse

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.BlobShape
import software.amazon.smithy.model.shapes.BooleanShape
import software.amazon.smithy.model.shapes.ByteShape
import software.amazon.smithy.model.shapes.IntegerShape
import software.amazon.smithy.model.shapes.LongShape
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ShortShape
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.TimestampShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.EventHeaderTrait
import software.amazon.smithy.model.traits.EventPayloadTrait
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.withBlock
import software.amazon.smithy.rust.codegen.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.smithy.generators.error.errorSymbol
import software.amazon.smithy.rust.codegen.smithy.protocols.Protocol
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticEventStreamUnionTrait
import software.amazon.smithy.rust.codegen.util.dq
import software.amazon.smithy.rust.codegen.util.expectTrait
import software.amazon.smithy.rust.codegen.util.hasTrait
import software.amazon.smithy.rust.codegen.util.toPascalCase

class EventStreamUnmarshallerGenerator(
    private val protocol: Protocol,
    private val model: Model,
    runtimeConfig: RuntimeConfig,
    private val symbolProvider: RustSymbolProvider,
    private val operationShape: OperationShape,
    private val unionShape: UnionShape,
) {
    private val unionSymbol = symbolProvider.toSymbol(unionShape)
    private val operationErrorSymbol = operationShape.errorSymbol(symbolProvider)
    private val smithyEventStream = CargoDependency.SmithyEventStream(runtimeConfig)
    private val codegenScope = arrayOf(
        "Blob" to RuntimeType("Blob", CargoDependency.SmithyTypes(runtimeConfig), "smithy_types"),
        "Error" to RuntimeType("Error", smithyEventStream, "smithy_eventstream::error"),
        "Header" to RuntimeType("Header", smithyEventStream, "smithy_eventstream::frame"),
        "HeaderValue" to RuntimeType("HeaderValue", smithyEventStream, "smithy_eventstream::frame"),
        "ExpectFns" to RuntimeType("smithy", smithyEventStream, "smithy_eventstream"),
        "Message" to RuntimeType("Message", smithyEventStream, "smithy_eventstream::frame"),
        "SmithyError" to RuntimeType("Error", CargoDependency.SmithyTypes(runtimeConfig), "smithy_types"),
        "UnmarshallMessage" to RuntimeType("UnmarshallMessage", smithyEventStream, "smithy_eventstream::frame"),
        "UnmarshalledMessage" to RuntimeType("UnmarshalledMessage", smithyEventStream, "smithy_eventstream::frame"),
    )

    fun render(): RuntimeType {
        val unmarshallerType = unionShape.eventStreamUnmarshallerType()
        return RuntimeType.forInlineFun("${unmarshallerType.name}::new", "event_stream_serde") { inlineWriter ->
            inlineWriter.renderUnmarshaller(unmarshallerType, unionSymbol)
        }
    }

    private fun RustWriter.renderUnmarshaller(unmarshallerType: RuntimeType, unionSymbol: Symbol) {
        rust(
            """
            ##[non_exhaustive]
            ##[derive(Debug)]
            pub struct ${unmarshallerType.name};

            impl ${unmarshallerType.name} {
                pub fn new() -> Self {
                    ${unmarshallerType.name}
                }
            }
            """
        )

        rustBlockTemplate(
            "impl #{UnmarshallMessage} for ${unmarshallerType.name}",
            *codegenScope
        ) {
            rust("type Output = #T;", unionSymbol)
            rust("type Error = #T;", operationErrorSymbol)

            rustBlockTemplate(
                """
                fn unmarshall(
                    &self,
                    message: &#{Message}
                ) -> std::result::Result<#{UnmarshalledMessage}<Self::Output, Self::Error>, #{Error}>
                """,
                *codegenScope
            ) {
                rustBlockTemplate(
                    """
                    let response_headers = #{ExpectFns}::parse_response_headers(&message)?;
                    match response_headers.message_type.as_str()
                    """,
                    *codegenScope
                ) {
                    rustBlock("\"event\" => ") {
                        renderUnmarshallEvent()
                    }
                    rustBlock("\"exception\" => ") {
                        renderUnmarshallError()
                    }
                    rustBlock("value => ") {
                        rustTemplate(
                            "return Err(#{Error}::Unmarshalling(format!(\"unrecognized :message-type: {}\", value)));",
                            *codegenScope
                        )
                    }
                }
            }
        }
    }

    private fun RustWriter.renderUnmarshallEvent() {
        rustBlock("match response_headers.smithy_type.as_str()") {
            for (member in unionShape.members()) {
                val target = model.expectShape(member.target, StructureShape::class.java)
                rustBlock("${member.memberName.dq()} => ") {
                    renderUnmarshallUnionMember(member, target)
                }
            }
            rustBlock("smithy_type => ") {
                // TODO: Handle this better once unions support unknown variants
                rustTemplate(
                    "return Err(#{Error}::Unmarshalling(format!(\"unrecognized :event-type: {}\", smithy_type)));",
                    *codegenScope
                )
            }
        }
    }

    private fun RustWriter.renderUnmarshallUnionMember(unionMember: MemberShape, unionStruct: StructureShape) {
        val unionMemberName = unionMember.memberName.toPascalCase()
        val payloadOnly =
            unionStruct.members().none { it.hasTrait<EventPayloadTrait>() || it.hasTrait<EventHeaderTrait>() }
        if (payloadOnly) {
            withBlock("let parsed = ", ";") {
                renderParseProtocolPayload(unionMember)
            }
            rustTemplate(
                "Ok(#{UnmarshalledMessage}::Event(#{Output}::$unionMemberName(parsed)))",
                "Output" to unionSymbol,
                *codegenScope
            )
        } else {
            rust("let mut builder = #T::builder();", symbolProvider.toSymbol(unionStruct))
            val payloadMember = unionStruct.members().firstOrNull { it.hasTrait<EventPayloadTrait>() }
            if (payloadMember != null) {
                renderUnmarshallEventPayload(payloadMember)
            }
            val headerMembers = unionStruct.members().filter { it.hasTrait<EventHeaderTrait>() }
            if (headerMembers.isNotEmpty()) {
                rustBlock("for header in message.headers()") {
                    rustBlock("match header.name().as_str()") {
                        for (member in headerMembers) {
                            rustBlock("${member.memberName.dq()} => ") {
                                renderUnmarshallEventHeader(member)
                            }
                        }
                        rust("_ => {}")
                    }
                }
            }
            rustTemplate(
                "Ok(#{UnmarshalledMessage}::Event(#{Output}::$unionMemberName(builder.build())))",
                "Output" to unionSymbol,
                *codegenScope
            )
        }
    }

    private fun RustWriter.renderUnmarshallEventHeader(member: MemberShape) {
        val memberName = symbolProvider.toMemberName(member)
        withBlock("builder = builder.$memberName(", ");") {
            when (val target = model.expectShape(member.target)) {
                is BooleanShape -> rustTemplate("#{ExpectFns}::expect_bool(header)?", *codegenScope)
                is ByteShape -> rustTemplate("#{ExpectFns}::expect_byte(header)?", *codegenScope)
                is ShortShape -> rustTemplate("#{ExpectFns}::expect_int16(header)?", *codegenScope)
                is IntegerShape -> rustTemplate("#{ExpectFns}::expect_int32(header)?", *codegenScope)
                is LongShape -> rustTemplate("#{ExpectFns}::expect_int64(header)?", *codegenScope)
                is BlobShape -> rustTemplate("#{ExpectFns}::expect_byte_array(header)?", *codegenScope)
                is StringShape -> rustTemplate("#{ExpectFns}::expect_string(header)?", *codegenScope)
                is TimestampShape -> rustTemplate("#{ExpectFns}::expect_timestamp(header)?", *codegenScope)
                else -> throw IllegalStateException("unsupported event stream header shape type: $target")
            }
        }
    }

    private fun RustWriter.renderUnmarshallEventPayload(member: MemberShape) {
        // TODO(EventStream): [RPC] Don't blow up on an initial-message that's not part of the union (:event-type will be "initial-request" or "initial-response")
        // TODO(EventStream): [RPC] Incorporate initial-message into original output (:event-type will be "initial-request" or "initial-response")
        val memberName = symbolProvider.toMemberName(member)
        withBlock("builder = builder.$memberName(", ");") {
            when (model.expectShape(member.target)) {
                is BlobShape -> {
                    rustTemplate("#{Blob}::new(message.payload().as_ref())", *codegenScope)
                }
                is StringShape -> {
                    rustTemplate(
                        """
                        std::str::from_utf8(message.payload())
                            .map_err(|_| #{Error}::Unmarshalling("message payload is not valid UTF-8".into()))?
                        """,
                        *codegenScope
                    )
                }
                is UnionShape, is StructureShape -> {
                    renderParseProtocolPayload(member)
                }
            }
        }
    }

    private fun RustWriter.renderParseProtocolPayload(member: MemberShape) {
        // TODO(EventStream): Check :content-type against expected content-type, error if unexpected
        val parser = protocol.structuredDataParser(operationShape).payloadParser(member)
        val memberName = member.memberName.toPascalCase()
        rustTemplate(
            """
                #{parser}(&message.payload()[..])
                    .map_err(|err| {
                        #{Error}::Unmarshalling(format!("failed to unmarshall $memberName: {}", err))
                    })?
            """,
            "parser" to parser,
            *codegenScope
        )
    }

    private fun RustWriter.renderUnmarshallError() {
        val syntheticUnion = unionShape.expectTrait<SyntheticEventStreamUnionTrait>()
        if (syntheticUnion.errorMembers.isNotEmpty()) {
            rustBlock("match response_headers.smithy_type.as_str()") {
                for (member in syntheticUnion.errorMembers) {
                    val target = model.expectShape(member.target, StructureShape::class.java)
                    rustBlock("${member.memberName.dq()} => ") {
                        val parser = protocol.structuredDataParser(operationShape).errorParser(target)
                        if (parser != null) {
                            rust("let mut builder = #T::builder();", symbolProvider.toSymbol(target))
                            // TODO(EventStream): Errors on the operation can be disjoint with errors in the union,
                            // so we need to generate a new top-level Error type for each event stream union.
                            rustTemplate(
                                """
                                builder = #{parser}(&message.payload()[..], builder)
                                    .map_err(|err| {
                                        #{Error}::Unmarshalling(format!("failed to unmarshall ${member.memberName}: {}", err))
                                    })?;
                                return Ok(#{UnmarshalledMessage}::Error(
                                    #{OpError}::new(
                                        #{OpError}Kind::${member.memberName.toPascalCase()}(builder.build()),
                                        #{SmithyError}::builder().build(),
                                    )
                                ))
                                """,
                                "OpError" to operationErrorSymbol,
                                "parser" to parser,
                                *codegenScope
                            )
                        }
                    }
                }
                rust("_ => {}")
            }
        }
        // TODO(EventStream): Generic error parsing; will need to refactor `parseGenericError` to
        // operate on bodies rather than responses. This should be easy for all but restJson,
        // which pulls the error type out of a header.
        rust("unimplemented!(\"event stream generic error parsing\")")
    }

    private fun UnionShape.eventStreamUnmarshallerType(): RuntimeType {
        val symbol = symbolProvider.toSymbol(this)
        return RuntimeType("${symbol.name.toPascalCase()}Unmarshaller", null, "crate::event_stream_serde")
    }
}
