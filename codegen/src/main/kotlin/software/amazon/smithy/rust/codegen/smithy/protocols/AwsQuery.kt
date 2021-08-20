/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.protocols

import software.amazon.smithy.aws.traits.protocols.AwsQueryErrorTrait
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.pattern.UriPattern
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ToShapeId
import software.amazon.smithy.model.traits.HttpTrait
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolConfig
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolGeneratorFactory
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolSupport
import software.amazon.smithy.rust.codegen.smithy.protocols.parse.AwsQueryParserGenerator
import software.amazon.smithy.rust.codegen.smithy.protocols.parse.StructuredDataParserGenerator
import software.amazon.smithy.rust.codegen.smithy.protocols.serialize.AwsQuerySerializerGenerator
import software.amazon.smithy.rust.codegen.smithy.protocols.serialize.StructuredDataSerializerGenerator
import software.amazon.smithy.rust.codegen.util.getTrait

class AwsQueryFactory : ProtocolGeneratorFactory<HttpBoundProtocolGenerator> {
    override fun buildProtocolGenerator(protocolConfig: ProtocolConfig): HttpBoundProtocolGenerator =
        HttpBoundProtocolGenerator(protocolConfig, AwsQueryProtocol(protocolConfig))

    override fun transformModel(model: Model): Model = model

    override fun support(): ProtocolSupport {
        return ProtocolSupport(
            requestSerialization = true,
            requestBodySerialization = true,
            responseDeserialization = true,
            errorDeserialization = true,
        )
    }
}

private val awsQueryHttpTrait = HttpTrait.builder()
    .code(200)
    .method("POST")
    .uri(UriPattern.parse("/"))
    .build()

class AwsQueryBindingResolver(private val model: Model) :
    StaticHttpBindingResolver(model, awsQueryHttpTrait, "application/x-www-form-urlencoded", "text/xml") {
    override fun errorCode(errorShape: ToShapeId): String {
        val error = model.expectShape(errorShape.toShapeId())
        return error.getTrait<AwsQueryErrorTrait>()?.code ?: errorShape.toShapeId().name
    }
}

class AwsQueryProtocol(private val protocolConfig: ProtocolConfig) : Protocol {
    private val runtimeConfig = protocolConfig.runtimeConfig
    private val awsQueryErrors: RuntimeType = RuntimeType.wrappedXmlErrors(runtimeConfig)
    override val httpBindingResolver: HttpBindingResolver = AwsQueryBindingResolver(protocolConfig.model)

    override val defaultTimestampFormat: TimestampFormatTrait.Format = TimestampFormatTrait.Format.DATE_TIME

    override fun structuredDataParser(operationShape: OperationShape): StructuredDataParserGenerator =
        AwsQueryParserGenerator(protocolConfig, awsQueryErrors)

    override fun structuredDataSerializer(operationShape: OperationShape): StructuredDataSerializerGenerator =
        AwsQuerySerializerGenerator(protocolConfig)

    override fun parseGenericError(operationShape: OperationShape): RuntimeType {
        return RuntimeType.forInlineFun("parse_generic_error", "xml_deser") {
            it.rustBlockTemplate(
                """
                pub fn parse_generic_error(
                    payload: &#{Bytes},
                    _http_status: Option<u16>,
                    _headers: Option<&#{HeaderMap}<#{HeaderValue}>>,
                ) -> Result<#{Error}, #{XmlError}>
                """,
                "Bytes" to RuntimeType.Bytes,
                "Error" to RuntimeType.GenericError(runtimeConfig),
                "HeaderMap" to RuntimeType.http.member("HeaderMap"),
                "HeaderValue" to RuntimeType.http.member("HeaderValue"),
                "XmlError" to CargoDependency.smithyXml(runtimeConfig).asType().member("decode::XmlError")
            ) {
                rust("#T::parse_generic_error(payload.as_ref())", awsQueryErrors)
            }
        }
    }
}
