/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.protocols.parse

import software.amazon.smithy.model.shapes.BlobShape
import software.amazon.smithy.model.shapes.BooleanShape
import software.amazon.smithy.model.shapes.CollectionShape
import software.amazon.smithy.model.shapes.DocumentShape
import software.amazon.smithy.model.shapes.MapShape
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.NumberShape
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.TimestampShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.EnumTrait
import software.amazon.smithy.model.traits.JsonNameTrait
import software.amazon.smithy.model.traits.SparseTrait
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.rustlang.Attribute
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.RustModule
import software.amazon.smithy.rust.codegen.rustlang.RustType
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.escape
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.withBlock
import software.amazon.smithy.rust.codegen.rustlang.withBlockTemplate
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.canUseDefault
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolConfig
import software.amazon.smithy.rust.codegen.smithy.generators.StructureGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.builderSymbol
import software.amazon.smithy.rust.codegen.smithy.generators.setterName
import software.amazon.smithy.rust.codegen.smithy.isBoxed
import software.amazon.smithy.rust.codegen.smithy.protocols.HttpBindingResolver
import software.amazon.smithy.rust.codegen.smithy.protocols.HttpLocation
import software.amazon.smithy.rust.codegen.smithy.protocols.deserializeFunctionName
import software.amazon.smithy.rust.codegen.util.dq
import software.amazon.smithy.rust.codegen.util.getTrait
import software.amazon.smithy.rust.codegen.util.hasTrait
import software.amazon.smithy.rust.codegen.util.outputShape
import software.amazon.smithy.rust.codegen.util.toPascalCase
import software.amazon.smithy.utils.StringUtils

class JsonParserGenerator(
    protocolConfig: ProtocolConfig,
    private val httpBindingResolver: HttpBindingResolver,
) : StructuredDataParserGenerator {
    private val model = protocolConfig.model
    private val symbolProvider = protocolConfig.symbolProvider
    private val runtimeConfig = protocolConfig.runtimeConfig
    private val smithyJson = CargoDependency.smithyJson(runtimeConfig).asType()
    private val jsonDeserModule = RustModule.private("json_deser")
    private val codegenScope = arrayOf(
        "Error" to smithyJson.member("deserialize::Error"),
        "ErrorReason" to smithyJson.member("deserialize::ErrorReason"),
        "expect_blob_or_null" to smithyJson.member("deserialize::token::expect_blob_or_null"),
        "expect_bool_or_null" to smithyJson.member("deserialize::token::expect_bool_or_null"),
        "expect_document" to smithyJson.member("deserialize::token::expect_document"),
        "expect_number_or_null" to smithyJson.member("deserialize::token::expect_number_or_null"),
        "expect_start_array" to smithyJson.member("deserialize::token::expect_start_array"),
        "expect_start_object" to smithyJson.member("deserialize::token::expect_start_object"),
        "expect_string_or_null" to smithyJson.member("deserialize::token::expect_string_or_null"),
        "expect_timestamp_or_null" to smithyJson.member("deserialize::token::expect_timestamp_or_null"),
        "json_token_iter" to smithyJson.member("deserialize::json_token_iter"),
        "Peekable" to RuntimeType.std.member("iter::Peekable"),
        "skip_value" to smithyJson.member("deserialize::token::skip_value"),
        "skip_to_end" to smithyJson.member("deserialize::token::skip_to_end"),
        "Token" to smithyJson.member("deserialize::Token"),
        "or_empty" to orEmptyJson()
    )

    override fun payloadParser(member: MemberShape): RuntimeType {
        val shape = model.expectShape(member.target)
        check(shape is UnionShape || shape is StructureShape) { "payload parser should only be used on structures & unions" }
        val fnName = symbolProvider.deserializeFunctionName(shape) + "_payload"
        return RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            it.rustBlockTemplate(
                "pub fn $fnName(input: &[u8]) -> Result<#{Shape}, #{Error}>",
                *codegenScope,
                "Shape" to symbolProvider.toSymbol(shape)
            ) {
                rustTemplate(
                    """
                    let mut tokens_owned = #{json_token_iter}(#{or_empty}(input)).peekable();
                    let tokens = &mut tokens_owned;
                    """,
                    *codegenScope
                )
                rust("let result =")
                deserializeMember(member)
                rustTemplate(".ok_or_else(|| #{Error}::custom(\"expected payload member value\"));", *codegenScope)
                expectEndOfTokenStream()
                rust("result")
            }
        }
    }

    override fun operationParser(operationShape: OperationShape): RuntimeType? {
        // Don't generate an operation JSON deserializer if there is no JSON body
        val httpDocumentMembers = httpBindingResolver.responseMembers(operationShape, HttpLocation.DOCUMENT)
        if (httpDocumentMembers.isEmpty()) {
            return null
        }

        val outputShape = operationShape.outputShape(model)
        val fnName = symbolProvider.deserializeFunctionName(operationShape)
        return RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            it.rustBlockTemplate(
                "pub fn $fnName(input: &[u8], mut builder: #{Builder}) -> Result<#{Builder}, #{Error}>",
                "Builder" to outputShape.builderSymbol(symbolProvider),
                *codegenScope
            ) {
                rustTemplate(
                    """
                    let mut tokens_owned = #{json_token_iter}(#{or_empty}(input)).peekable();
                    let tokens = &mut tokens_owned;
                    #{expect_start_object}(tokens.next())?;
                    """,
                    *codegenScope
                )
                deserializeStructInner(httpDocumentMembers)
                expectEndOfTokenStream()
                rust("Ok(builder)")
            }
        }
    }

    override fun errorParser(errorShape: StructureShape): RuntimeType? {
        if (errorShape.members().isEmpty()) {
            return null
        }
        val fnName = symbolProvider.deserializeFunctionName(errorShape) + "json_err"
        return RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            it.rustBlockTemplate(
                "pub fn $fnName(input: &[u8], mut builder: #{Builder}) -> Result<#{Builder}, #{Error}>",
                "Builder" to errorShape.builderSymbol(symbolProvider),
                *codegenScope
            ) {
                rustTemplate(
                    """
                    let mut tokens_owned = #{json_token_iter}(#{or_empty}(input)).peekable();
                    let tokens = &mut tokens_owned;
                    #{expect_start_object}(tokens.next())?;
                    """,
                    *codegenScope
                )
                deserializeStructInner(errorShape.members())
                expectEndOfTokenStream()
                rust("Ok(builder)")
            }
        }
    }

    override fun documentParser(operationShape: OperationShape): RuntimeType {
        val fnName = "parse_document"
        return RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            it.rustBlockTemplate(
                "pub fn $fnName(input: &[u8]) -> Result<#{Document}, #{Error}>",
                "Document" to RuntimeType.Document(runtimeConfig),
                *codegenScope,
            ) {
                rustTemplate(
                    """
                    let mut tokens_owned = #{json_token_iter}(input).peekable();
                    let tokens = &mut tokens_owned;
                    """,
                    *codegenScope
                )
                rustTemplate("let result = #{expect_document}(tokens);", *codegenScope)
                expectEndOfTokenStream()
                rust("result")
            }
        }
    }

    private fun orEmptyJson(): RuntimeType = RuntimeType.forInlineFun("or_empty_doc", jsonDeserModule) {
        it.rust(
            """
            pub fn or_empty_doc(data: &[u8]) -> &[u8] {
                if data.is_empty() {
                    b"{}"
                } else {
                    data
                }
            }
        """
        )
    }

    fun renderStructure(
            writer: RustWriter,
            structureShape: StructureShape,
            includedMembers: List<MemberShape>,
    ) {
        val fnName = symbolProvider.deserializeFunctionName(structureShape)
        val unusedMut = if (includedMembers.isEmpty()) "##[allow(unused_mut)] " else ""
        writer.write("")
        writer.rustBlockTemplate(
                "##[allow(dead_code)] pub fn $fnName(input: &[u8], ${unusedMut}mut builder: #{Builder}) -> Result<#{Builder}, #{Error}>",
                *codegenScope,
                "Builder" to structureShape.builderSymbol(symbolProvider),
        ) {
            rustTemplate(
                    """
                    let mut tokens_owned = #{json_token_iter}(#{or_empty}(input)).peekable();
                    let tokens = &mut tokens_owned;
                    #{expect_start_object}(tokens.next())?;
                """.trimIndent(),
                    *codegenScope
            )
            deserializeStructInner(includedMembers)
            expectEndOfTokenStream()
            rust("Ok(builder)")
        }
    }

    private fun RustWriter.expectEndOfTokenStream() {
        rustBlock("if tokens.next().is_some()") {
            rustTemplate(
                "return Err(#{Error}::custom(\"found more JSON tokens after completing parsing\"));",
                *codegenScope
            )
        }
    }

    private fun RustWriter.deserializeStructInner(members: Collection<MemberShape>) {
        objectKeyLoop(hasMembers = members.isNotEmpty()) {
            rustBlock("match key.to_unescaped()?.as_ref()") {
                for (member in members) {
                    rustBlock("${member.wireName().dq()} =>") {
                        withBlock("builder = builder.${member.setterName()}(", ");") {
                            deserializeMember(member)
                        }
                    }
                }
                rustTemplate("_ => #{skip_value}(tokens)?", *codegenScope)
            }
        }
    }

    private fun RustWriter.deserializeMember(memberShape: MemberShape) {
        when (val target = model.expectShape(memberShape.target)) {
            is StringShape -> deserializeString(target)
            is BooleanShape -> rustTemplate("#{expect_bool_or_null}(tokens.next())?", *codegenScope)
            is NumberShape -> deserializeNumber(target)
            is BlobShape -> rustTemplate("#{expect_blob_or_null}(tokens.next())?", *codegenScope)
            is TimestampShape -> deserializeTimestamp(memberShape)
            is CollectionShape -> deserializeCollection(target)
            is MapShape -> deserializeMap(target)
            is StructureShape -> deserializeStruct(target)
            is UnionShape -> deserializeUnion(target)
            is DocumentShape -> rustTemplate("Some(#{expect_document}(tokens)?)", *codegenScope)
            else -> TODO(target.toString())
        }
        val symbol = symbolProvider.toSymbol(memberShape)
        if (symbol.isBoxed()) {
            rust(".map(Box::new)")
        }
    }

    private fun RustWriter.deserializeStringInner(target: StringShape, escapedStrName: String) {
        withBlock("$escapedStrName.to_unescaped().map(|u|", ")") {
            when (target.hasTrait<EnumTrait>()) {
                true -> rust("#T::from(u.as_ref())", symbolProvider.toSymbol(target))
                else -> rust("u.into_owned()")
            }
        }
    }

    private fun RustWriter.deserializeString(target: StringShape) {
        withBlockTemplate("#{expect_string_or_null}(tokens.next())?.map(|s|", ").transpose()?", *codegenScope) {
            deserializeStringInner(target, "s")
        }
    }

    private fun RustWriter.deserializeNumber(target: NumberShape) {
        val symbol = symbolProvider.toSymbol(target)
        rustTemplate("#{expect_number_or_null}(tokens.next())?.map(|v| v.to_#{T}())", "T" to symbol, *codegenScope)
    }

    private fun RustWriter.deserializeTimestamp(member: MemberShape) {
        val timestampFormat =
            httpBindingResolver.timestampFormat(
                member, HttpLocation.DOCUMENT,
                TimestampFormatTrait.Format.EPOCH_SECONDS
            )
        val timestampFormatType = RuntimeType.TimestampFormat(runtimeConfig, timestampFormat)
        rustTemplate("#{expect_timestamp_or_null}(tokens.next(), #{T})?", "T" to timestampFormatType, *codegenScope)
    }

    private fun RustWriter.deserializeCollection(shape: CollectionShape) {
        val fnName = symbolProvider.deserializeFunctionName(shape)
        val isSparse = shape.hasTrait<SparseTrait>()
        val parser = RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            // Allow non-snake-case since some SDK models have lists with names prefixed with `__listOf__`,
            // which become `__list_of__`, and the Rust compiler warning doesn't like multiple adjacent underscores.
            it.rustBlockTemplate(
                """
                ##[allow(clippy::type_complexity, non_snake_case)]
                pub fn $fnName<'a, I>(tokens: &mut #{Peekable}<I>) -> Result<Option<#{Shape}>, #{Error}>
                    where I: Iterator<Item = Result<#{Token}<'a>, #{Error}>>
                """,
                "Shape" to symbolProvider.toSymbol(shape),
                *codegenScope,
            ) {
                startArrayOrNull {
                    rust("let mut items = Vec::new();")
                    rustBlock("loop") {
                        rustBlock("match tokens.peek()") {
                            rustBlockTemplate("Some(Ok(#{Token}::EndArray { .. })) =>", *codegenScope) {
                                rust("tokens.next().transpose().unwrap(); break;")
                            }
                            rustBlock("_ => ") {
                                if (isSparse) {
                                    withBlock("items.push(", ");") {
                                        deserializeMember(shape.member)
                                    }
                                } else {
                                    withBlock("let value =", ";") {
                                        deserializeMember(shape.member)
                                    }
                                    rustBlock("if let Some(value) = value") {
                                        rust("items.push(value);")
                                    }
                                }
                            }
                        }
                    }
                    rust("Ok(Some(items))")
                }
            }
        }
        rust("#T(tokens)?", parser)
    }

    private fun RustWriter.deserializeMap(shape: MapShape) {
        val keyTarget = model.expectShape(shape.key.target) as StringShape
        val fnName = symbolProvider.deserializeFunctionName(shape)
        val isSparse = shape.hasTrait<SparseTrait>()
        val parser = RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            // Allow non-snake-case since some SDK models have maps with names prefixed with `__mapOf__`,
            // which become `__map_of__`, and the Rust compiler warning doesn't like multiple adjacent underscores.
            it.rustBlockTemplate(
                """
                ##[allow(clippy::type_complexity, non_snake_case)]
                pub fn $fnName<'a, I>(tokens: &mut #{Peekable}<I>) -> Result<Option<#{Shape}>, #{Error}>
                    where I: Iterator<Item = Result<#{Token}<'a>, #{Error}>>
                """,
                "Shape" to symbolProvider.toSymbol(shape),
                *codegenScope,
            ) {
                startObjectOrNull {
                    rust("let mut map = #T::new();", RustType.HashMap.RuntimeType)
                    objectKeyLoop(hasMembers = true) {
                        withBlock("let key =", "?;") {
                            deserializeStringInner(keyTarget, "key")
                        }
                        withBlock("let value =", ";") {
                            deserializeMember(shape.value)
                        }
                        if (isSparse) {
                            rust("map.insert(key, value);")
                        } else {
                            rustBlock("if let Some(value) = value") {
                                rust("map.insert(key, value);")
                            }
                        }
                    }
                    rust("Ok(Some(map))")
                }
            }
        }
        rust("#T(tokens)?", parser)
    }

    private fun RustWriter.deserializeStruct(shape: StructureShape) {
        val fnName = symbolProvider.deserializeFunctionName(shape)
        val symbol = symbolProvider.toSymbol(shape)
        val nestedParser = RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            it.rustBlockTemplate(
                """
                pub fn $fnName<'a, I>(tokens: &mut #{Peekable}<I>) -> Result<Option<#{Shape}>, #{Error}>
                    where I: Iterator<Item = Result<#{Token}<'a>, #{Error}>>
                """,
                "Shape" to symbol,
                *codegenScope,
            ) {
                startObjectOrNull {
                    Attribute.AllowUnusedMut.render(this)
                    rustTemplate("let mut builder = #{Shape}::builder();", *codegenScope, "Shape" to symbol)
                    deserializeStructInner(shape.members())
                    withBlock("Ok(Some(builder.build()", "))") {
                        if (StructureGenerator.fallibleBuilder(shape, symbolProvider)) {
                            rustTemplate(
                                ".map_err(|err| #{Error}::new(#{ErrorReason}::Custom(" +
                                    "format!(\"{}\", err).into()), None))?",
                                *codegenScope
                            )
                        }
                    }
                }
            }
        }
        rust("#T(tokens)?", nestedParser)
    }

    private fun RustWriter.deserializeUnion(shape: UnionShape) {
        val fnName = symbolProvider.deserializeFunctionName(shape)
        val symbol = symbolProvider.toSymbol(shape)
        val nestedParser = RuntimeType.forInlineFun(fnName, jsonDeserModule) {
            it.rustBlockTemplate(
                """
                pub fn $fnName<'a, I>(tokens: &mut #{Peekable}<I>) -> Result<Option<#{Shape}>, #{Error}>
                    where I: Iterator<Item = Result<#{Token}<'a>, #{Error}>>
                """,
                *codegenScope,
                "Shape" to symbol
            ) {
                rust("let mut variant = None;")
                rustBlock("match tokens.next().transpose()?") {
                    rustBlockTemplate(
                        """
                        Some(#{Token}::ValueNull { .. }) => return Ok(None),
                        Some(#{Token}::StartObject { .. }) =>
                        """,
                        *codegenScope
                    ) {
                        objectKeyLoop(hasMembers = shape.members().isNotEmpty()) {
                            rustTemplate(
                                """
                                if variant.is_some() {
                                    return Err(#{Error}::custom("encountered mixed variants in union"));
                                }
                                """,
                                *codegenScope
                            )
                            withBlock("variant = match key.to_unescaped()?.as_ref() {", "};") {
                                for (member in shape.members()) {
                                    val variantName = member.memberName.toPascalCase()
                                    rustBlock("${member.wireName().dq()} =>") {
                                        withBlock("Some(#T::$variantName(", "))", symbol) {
                                            deserializeMember(member)
                                            unwrapOrDefaultOrError(member)
                                        }
                                    }
                                }
                                // TODO: Handle unrecognized union variants (https://github.com/awslabs/smithy-rs/issues/185)
                                rust("_ => None")
                            }
                        }
                    }
                    rustTemplate(
                        "_ => return Err(#{Error}::custom(\"expected start object or null\"))",
                        *codegenScope
                    )
                }
                rust("Ok(variant)")
            }
        }
        rust("#T(tokens)?", nestedParser)
    }

    private fun RustWriter.unwrapOrDefaultOrError(member: MemberShape) {
        if (symbolProvider.toSymbol(member).canUseDefault()) {
            rust(".unwrap_or_default()")
        } else {
            rustTemplate(
                ".ok_or_else(|| #{Error}::custom(\"value for '${escape(member.memberName)}' cannot be null\"))?",
                *codegenScope
            )
        }
    }

    private fun RustWriter.objectKeyLoop(hasMembers: Boolean, inner: RustWriter.() -> Unit) {
        if (!hasMembers) {
            rustTemplate("#{skip_to_end}(tokens)?;", *codegenScope)
        } else {
            rustBlock("loop") {
                rustBlock("match tokens.next().transpose()?") {
                    rustBlockTemplate(
                        """
                        Some(#{Token}::EndObject { .. }) => break,
                        Some(#{Token}::ObjectKey { key, .. }) =>
                        """,
                        *codegenScope
                    ) {
                        inner()
                    }
                    rustTemplate(
                        "_ => return Err(#{Error}::custom(\"expected object key or end object\"))",
                        *codegenScope
                    )
                }
            }
        }
    }

    private fun RustWriter.startArrayOrNull(inner: RustWriter.() -> Unit) = startOrNull("array", inner)
    private fun RustWriter.startObjectOrNull(inner: RustWriter.() -> Unit) = startOrNull("object", inner)
    private fun RustWriter.startOrNull(objectOrArray: String, inner: RustWriter.() -> Unit) {
        rustBlockTemplate("match tokens.next().transpose()?", *codegenScope) {
            rustBlockTemplate(
                """
                    Some(#{Token}::ValueNull { .. }) => Ok(None),
                    Some(#{Token}::Start${StringUtils.capitalize(objectOrArray)} { .. }) =>
                """,
                *codegenScope
            ) {
                inner()
            }
            rustBlockTemplate("_ =>") {
                rustTemplate(
                    "Err(#{Error}::custom(\"expected start $objectOrArray or null\"))",
                    *codegenScope
                )
            }
        }
    }

    private fun MemberShape.wireName(): String = getTrait<JsonNameTrait>()?.value ?: memberName
}
