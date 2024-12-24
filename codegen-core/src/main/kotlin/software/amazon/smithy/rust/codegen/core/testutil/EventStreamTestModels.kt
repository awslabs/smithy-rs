/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.core.testutil

import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.dataformat.cbor.CBORFactory
import software.amazon.smithy.model.Model
import software.amazon.smithy.rust.codegen.core.smithy.CodegenContext
import software.amazon.smithy.rust.codegen.core.smithy.protocols.AwsJson
import software.amazon.smithy.rust.codegen.core.smithy.protocols.AwsJsonVersion
import software.amazon.smithy.rust.codegen.core.smithy.protocols.Protocol
import software.amazon.smithy.rust.codegen.core.smithy.protocols.RestJson
import software.amazon.smithy.rust.codegen.core.smithy.protocols.RestXml
import software.amazon.smithy.rust.codegen.core.smithy.protocols.RpcV2Cbor
import java.util.Base64

private fun fillInBaseModel(
    namespacedProtocolName: String,
    extraServiceAnnotations: String = "",
): String =
    """
    namespace test

    use smithy.framework#ValidationException
    use $namespacedProtocolName

    union TestUnion {
        Foo: String,
        Bar: Integer,
    }
    structure TestStruct {
        someString: String,
        someInt: Integer,
    }

    @error("client")
    structure SomeError {
        Message: String,
    }

    structure MessageWithBlob { @eventPayload data: Blob }
    structure MessageWithString { @eventPayload data: String }
    structure MessageWithStruct { @eventPayload someStruct: TestStruct }
    structure MessageWithUnion { @eventPayload someUnion: TestUnion }
    structure MessageWithHeaders {
        @eventHeader blob: Blob,
        @eventHeader boolean: Boolean,
        @eventHeader byte: Byte,
        @eventHeader int: Integer,
        @eventHeader long: Long,
        @eventHeader short: Short,
        @eventHeader string: String,
        @eventHeader timestamp: Timestamp,
    }
    structure MessageWithHeaderAndPayload {
        @eventHeader header: String,
        @eventPayload payload: Blob,
    }
    structure MessageWithNoHeaderPayloadTraits {
        someInt: Integer,
        someString: String,
    }

    @streaming
    union TestStream {
        MessageWithBlob: MessageWithBlob,
        MessageWithString: MessageWithString,
        MessageWithStruct: MessageWithStruct,
        MessageWithUnion: MessageWithUnion,
        MessageWithHeaders: MessageWithHeaders,
        MessageWithHeaderAndPayload: MessageWithHeaderAndPayload,
        MessageWithNoHeaderPayloadTraits: MessageWithNoHeaderPayloadTraits,
        SomeError: SomeError,
    }

    structure TestStreamInputOutput {
        @required
        @httpPayload
        value: TestStream
    }

    @http(method: "POST", uri: "/test")
    operation TestStreamOp {
        input: TestStreamInputOutput,
        output: TestStreamInputOutput,
        errors: [SomeError, ValidationException],
    }

    $extraServiceAnnotations
    @${namespacedProtocolName.substringAfter("#")}
    service TestService { version: "123", operations: [TestStreamOp] }
    """

object EventStreamTestModels {
    private fun restJson1(): Model = fillInBaseModel("aws.protocols#restJson1").asSmithyModel()

    private fun restXml(): Model = fillInBaseModel("aws.protocols#restXml").asSmithyModel()

    private fun awsJson11(): Model = fillInBaseModel("aws.protocols#awsJson1_1").asSmithyModel()

    private fun rpcv2Cbor(): Model = fillInBaseModel("smithy.protocols#rpcv2Cbor").asSmithyModel()

    private fun awsQuery(): Model =
        fillInBaseModel("aws.protocols#awsQuery", "@xmlNamespace(uri: \"https://example.com\")").asSmithyModel()

    private fun ec2Query(): Model =
        fillInBaseModel("aws.protocols#ec2Query", "@xmlNamespace(uri: \"https://example.com\")").asSmithyModel()

    data class TestCase(
        val protocolShapeId: String,
        val model: Model,
        val mediaType: String,
        val requestContentType: String,
        val responseContentType: String,
        val eventStreamMessageContentType: String,
        val validTestStruct: String,
        val validMessageWithNoHeaderPayloadTraits: String,
        val validTestUnion: String,
        val validSomeError: String,
        val validUnmodeledError: String,
        val protocolBuilder: (CodegenContext) -> Protocol,
    ) {
        override fun toString(): String = protocolShapeId
    }

    private fun base64Encode(input: ByteArray): String {
        val encodedBytes = Base64.getEncoder().encode(input)
        return String(encodedBytes)
    }

    private fun createCborFromJson(jsonString: String): ByteArray {
        val jsonMapper = ObjectMapper()
        val cborMapper = ObjectMapper(CBORFactory())
        // Parse JSON string to a generic type.
        val jsonData = jsonMapper.readValue(jsonString, Any::class.java)
        // Convert the parsed data to CBOR.
        return cborMapper.writeValueAsBytes(jsonData)
    }

    private val restJsonTestCase =
        TestCase(
            protocolShapeId = "aws.protocols#restJson1",
            model = restJson1(),
            mediaType = "application/json",
            requestContentType = "application/vnd.amazon.eventstream",
            responseContentType = "application/json",
            eventStreamMessageContentType = "application/json",
            validTestStruct = """{"someString":"hello","someInt":5}""",
            validMessageWithNoHeaderPayloadTraits = """{"someString":"hello","someInt":5}""",
            validTestUnion = """{"Foo":"hello"}""",
            validSomeError = """{"Message":"some error"}""",
            validUnmodeledError = """{"Message":"unmodeled error"}""",
        ) { RestJson(it) }

    val TEST_CASES =
        listOf(
            //
            // restJson1
            //
            restJsonTestCase,
            //
            // rpcV2Cbor
            //
            restJsonTestCase.copy(
                protocolShapeId = "smithy.protocols#rpcv2Cbor",
                model = rpcv2Cbor(),
                mediaType = "application/cbor",
                responseContentType = "application/cbor",
                eventStreamMessageContentType = "application/cbor",
                validTestStruct = base64Encode(createCborFromJson(restJsonTestCase.validTestStruct)),
                validMessageWithNoHeaderPayloadTraits = base64Encode(createCborFromJson(restJsonTestCase.validMessageWithNoHeaderPayloadTraits)),
                validTestUnion = base64Encode(createCborFromJson(restJsonTestCase.validTestUnion)),
                validSomeError = base64Encode(createCborFromJson(restJsonTestCase.validSomeError)),
                validUnmodeledError = base64Encode(createCborFromJson(restJsonTestCase.validUnmodeledError)),
                protocolBuilder = { RpcV2Cbor(it) },
            ),
            //
            // awsJson1_1
            //
            restJsonTestCase.copy(
                protocolShapeId = "aws.protocols#awsJson1_1",
                model = awsJson11(),
                mediaType = "application/x-amz-json-1.1",
                requestContentType = "application/x-amz-json-1.1",
                responseContentType = "application/x-amz-json-1.1",
                eventStreamMessageContentType = "application/json",
            ) { AwsJson(it, AwsJsonVersion.Json11) },
            //
            // restXml
            //
            TestCase(
                protocolShapeId = "aws.protocols#restXml",
                model = restXml(),
                mediaType = "application/xml",
                requestContentType = "application/vnd.amazon.eventstream",
                responseContentType = "application/xml",
                eventStreamMessageContentType = "application/xml",
                validTestStruct =
                    """
                    <TestStruct>
                        <someString>hello</someString>
                        <someInt>5</someInt>
                    </TestStruct>
                    """.trimIndent(),
                validMessageWithNoHeaderPayloadTraits =
                    """
                    <MessageWithNoHeaderPayloadTraits>
                        <someString>hello</someString>
                        <someInt>5</someInt>
                    </MessageWithNoHeaderPayloadTraits>
                    """.trimIndent(),
                validTestUnion = "<TestUnion><Foo>hello</Foo></TestUnion>",
                validSomeError =
                    """
                    <ErrorResponse>
                        <Error>
                            <Type>SomeError</Type>
                            <Code>SomeError</Code>
                            <Message>some error</Message>
                        </Error>
                    </ErrorResponse>
                    """.trimIndent(),
                validUnmodeledError =
                    """
                    <ErrorResponse>
                        <Error>
                            <Type>UnmodeledError</Type>
                            <Code>UnmodeledError</Code>
                            <Message>unmodeled error</Message>
                        </Error>
                    </ErrorResponse>
                    """.trimIndent(),
            ) { RestXml(it) },
        )
}
