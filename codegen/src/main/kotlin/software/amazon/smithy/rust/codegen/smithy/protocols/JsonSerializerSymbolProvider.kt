/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.protocols

import software.amazon.smithy.codegen.core.Symbol
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.knowledge.HttpBinding
import software.amazon.smithy.model.knowledge.HttpBindingIndex
import software.amazon.smithy.model.shapes.BlobShape
import software.amazon.smithy.model.shapes.DocumentShape
import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.JsonNameTrait
import software.amazon.smithy.model.traits.TimestampFormatTrait
import software.amazon.smithy.rust.codegen.rustlang.Attribute
import software.amazon.smithy.rust.codegen.rustlang.Custom
import software.amazon.smithy.rust.codegen.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.rustlang.RustType
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.Writable
import software.amazon.smithy.rust.codegen.rustlang.contains
import software.amazon.smithy.rust.codegen.rustlang.render
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.stripOuter
import software.amazon.smithy.rust.codegen.rustlang.withBlock
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.smithy.SymbolMetadataProvider
import software.amazon.smithy.rust.codegen.smithy.expectRustMetadata
import software.amazon.smithy.rust.codegen.smithy.isOptional
import software.amazon.smithy.rust.codegen.smithy.letIf
import software.amazon.smithy.rust.codegen.smithy.rustType
import software.amazon.smithy.rust.codegen.smithy.traits.InputBodyTrait
import software.amazon.smithy.rust.codegen.smithy.traits.OutputBodyTrait
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticInputTrait
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticOutputTrait
import software.amazon.smithy.rust.codegen.util.dq

/**
 * JsonSerializerSymbolProvider annotates shapes and members with `serde` attributes
 */
class JsonSerializerSymbolProvider(
    private val model: Model,
    private val base: RustSymbolProvider,
    defaultTimestampFormat: TimestampFormatTrait.Format
) :
    SymbolMetadataProvider(base) {

    data class SerdeConfig(val serialize: Boolean, val deserialize: Boolean)

    private fun MemberShape.serializedName() =
        this.getTrait(JsonNameTrait::class.java).map { it.value }.orElse(this.memberName)

    private val serializerBuilder = SerializerBuilder(base, model, defaultTimestampFormat)
    override fun memberMeta(memberShape: MemberShape): RustMetadata {
        val currentMeta = base.toSymbol(memberShape).expectRustMetadata()
        val serdeConfig = serdeRequired(model.expectShape(memberShape.container))
        val attribs = mutableListOf<Attribute>()
        if (serdeConfig.serialize || serdeConfig.deserialize) {
            attribs.add(Custom("serde(rename = ${memberShape.serializedName().dq()})"))
        }
        if (serdeConfig.serialize) {
            if (base.toSymbol(memberShape).rustType().stripOuter<RustType.Reference>() is RustType.Option) {
                attribs.add(Custom("serde(skip_serializing_if = \"Option::is_none\")"))
            }
            serializerBuilder.serializerFor(memberShape)?.also {
                attribs.add(Custom("serde(serialize_with = ${it.fullyQualifiedName().dq()})", listOf(it)))
            }
        }
        if (serdeConfig.deserialize) {
            serializerBuilder.deserializerFor(memberShape)?.also {
                attribs.add(Custom("serde(deserialize_with = ${it.fullyQualifiedName().dq()})", listOf(it)))
            }
            if (model.expectShape(memberShape.container) is StructureShape && base.toSymbol(memberShape).isOptional()
            ) {
                attribs.add(Custom("serde(default)"))
            }
        }
        return currentMeta.copy(additionalAttributes = currentMeta.additionalAttributes + attribs)
    }

    override fun structureMeta(structureShape: StructureShape): RustMetadata = containerMeta(structureShape)
    override fun unionMeta(unionShape: UnionShape): RustMetadata = containerMeta(unionShape)
    override fun enumMeta(stringShape: StringShape): RustMetadata = containerMeta(stringShape)

    private fun containerMeta(container: Shape): RustMetadata {
        val currentMeta = base.toSymbol(container).expectRustMetadata()
        val requiredSerde = serdeRequired(container)
        return currentMeta
            .letIf(requiredSerde.serialize) { it.withDerives(RuntimeType.Serialize) }
            .letIf(requiredSerde.deserialize) { it.withDerives(RuntimeType.Deserialize) }
    }

    private fun serdeRequired(shape: Shape): SerdeConfig {
        return when {
            shape.hasTrait(InputBodyTrait::class.java) -> SerdeConfig(serialize = true, deserialize = false)
            shape.hasTrait(OutputBodyTrait::class.java) -> SerdeConfig(serialize = false, deserialize = true)

            // The bodies must be serializable. The top level inputs are _not_
            shape.hasTrait(SyntheticInputTrait::class.java) -> SerdeConfig(serialize = false, deserialize = false)
            shape.hasTrait(SyntheticOutputTrait::class.java) -> SerdeConfig(serialize = false, deserialize = false)
            else -> SerdeConfig(serialize = true, deserialize = true)
        }
    }
}

class SerializerBuilder(
    private val symbolProvider: RustSymbolProvider,
    model: Model,
    private val defaultTimestampFormat: TimestampFormatTrait.Format
) {
    private val inp = "_inp"
    private val ser = "_serializer"
    private val httpBindingIndex = HttpBindingIndex.of(model)
    private val runtimeConfig = symbolProvider.config().runtimeConfig

    // Small hack to get the Rust type for these problematic shapes
    private val instant = RuntimeType.Instant(runtimeConfig).toSymbol().rustType()
    private val blob = symbolProvider.toSymbol(BlobShape.builder().id("dummy#blob").build()).rustType()
    private val document = symbolProvider.toSymbol(DocumentShape.builder().id("dummy#doc").build()).rustType()
    private val customShapes = setOf(instant, blob, document)

    private val handWrittenSerializers: Map<String, (RustWriter) -> Unit> = mapOf(
        "stdoptionoptionblob_ser" to { writer ->

            writer.rustBlock("match $inp") {
                write(
                    "Some(blob) => $ser.serialize_str(&#T(blob.as_ref())),",
                    RuntimeType.Base64Encode(runtimeConfig)
                )
                write("None => $ser.serialize_none()")
            }
        },
        "blob_ser" to { writer ->
            writer.write(
                "$ser.serialize_str(&#T($inp.as_ref()))",
                RuntimeType.Base64Encode(runtimeConfig)
            )
        },
        "stdoptionoptioninstant_http_date_ser" to { writer ->
            val timestampFormatType = RuntimeType.TimestampFormat(runtimeConfig, TimestampFormatTrait.Format.HTTP_DATE)
            writer.rustBlock("match $inp") {
                write(
                    "Some(ts) => $ser.serialize_some(&ts.fmt(#T)),", timestampFormatType
                )
                write("None => _serializer.serialize_none()")
            }
        },
        "stdoptionoptioninstant_date_time_ser" to { writer ->
            val timestampFormatType = RuntimeType.TimestampFormat(runtimeConfig, TimestampFormatTrait.Format.DATE_TIME)
            writer.rustBlock("match $inp") {
                write(
                    "Some(ts) => $ser.serialize_some(&ts.fmt(#T)),", timestampFormatType
                )
                write("None => _serializer.serialize_none()")
            }
        },
        "stdoptionoptioninstant_epoch_seconds_ser" to { writer ->
            writer.rustTemplate(
                """
                use #{serde};
                $inp.map(#{instant_epoch}::InstantEpoch).serialize($ser)
            """,
                "serde" to RuntimeType.Serialize, "instant_epoch" to RuntimeType.InstantEpoch
            )
        },
        "instant_epoch_seconds_ser" to { writer ->
            writer.write("use #T;", RuntimeType.Serialize)
            writer.write("#T::InstantEpoch(*$inp).serialize($ser)", RuntimeType.InstantEpoch)
        },
        "document_ser" to { writer ->
            writer.write("use #T;", RuntimeType.Serialize)
            writer.write("#T::SerDoc($inp).serialize($ser)", RuntimeType.DocJson)
        }
    )

    /**
     * Generate a deserializer for the given type dynamically, eg:
     * ```rust
     *  use ::serde::Deserialize;
     *  Ok(
     *      Option::<crate::instant_epoch::InstantEpoch>::deserialize(_deser)?
     *          .map(|el| el.0)
     *  )
     * ```
     *
     * It utilizes a newtype that defines the given serialization to access the serde serializer
     * then performs any necessary mapping / unmapping. This has a slight disadvantage in that
     * that wrapping structures like `Vec` may be allocated twice—I think we should be able to avoid
     * this eventually however.
     */
    private fun RustWriter.deserializer(t: RustType, memberShape: MemberShape) {
        write("use #T;", RuntimeType.Deserialize)
        withBlock("Ok(", ")") {
            serdeType(t, memberShape)(this)
            write("::deserialize(_deser)?")
            unrollDeser(t)
        }
    }

    private fun RustWriter.unrollDeser(realType: RustType) {
        when (realType) {
            is RustType.Vec -> withBlock(".into_iter().map(|el|el", ").collect()") {
                unrollDeser(realType.member)
            }
            is RustType.Option -> withBlock(".map(|el|el", ")") {
                unrollDeser(realType.member)
            }

            is RustType.HashMap -> withBlock(".into_iter().map(|(k,el)|(k, el", ")).collect()") {
                unrollDeser(realType.member)
            }

            // We will only create HashSets of strings, so we shouldn't ever hit this
            is RustType.HashSet -> TODO("https://github.com/awslabs/smithy-rs/issues/44")

            is RustType.Box -> {
                unrollDeser(realType.member)
                write(".into()")
            }

            else -> if (customShapes.contains(realType)) {
                write(".0")
            } else {
                TODO("unsupported type $realType")
            }
        }
    }

    private fun RustWriter.serdeContainerType(realType: RustType.Container, memberShape: MemberShape) {
        val prefix = when (realType) {
            is RustType.HashMap -> "${realType.namespace}::${realType.name}::<String, "
            else -> "${realType.namespace}::${realType.name}::<"
        }
        withBlock(prefix, ">") {
            serdeType(realType.member, memberShape)(this)
        }
    }

    private fun serdeType(realType: RustType, memberShape: MemberShape): Writable {
        return when (realType) {
            instant -> writable {
                val format = tsFormat(memberShape)
                when (format) {
                    TimestampFormatTrait.Format.DATE_TIME -> write("#T::InstantIso8601", RuntimeType.Instant8601)
                    TimestampFormatTrait.Format.EPOCH_SECONDS -> write("#T::InstantEpoch", RuntimeType.InstantEpoch)
                    TimestampFormatTrait.Format.HTTP_DATE -> write(
                        "#T::InstantHttpDate",
                        RuntimeType.InstantHttpDate
                    )
                    else -> write("todo!() /* unknown timestamp format */")
                }
            }
            blob -> writable { write("#T::BlobDeser", RuntimeType.BlobSerde(runtimeConfig)) }
            is RustType.Container -> writable { serdeContainerType(realType, memberShape) }
            else -> TODO("Serializing $realType is not supported")
        }
    }

    /** correct argument type for the serde custom serializer */
    private fun serializerType(symbol: Symbol): Symbol {
        val unref = symbol.rustType().stripOuter<RustType.Reference>()

        // Convert `Vec<T>` to `[T]` when present. This is needed to avoid
        // Clippy complaining (and is also better in general).
        val outType = when (unref) {
            is RustType.Vec -> RustType.Slice(unref.member)
            else -> unref
        }
        val referenced = RustType.Reference(member = outType, lifetime = null)
        return symbol.toBuilder().rustType(referenced).build()
    }

    private fun tsFormat(memberShape: MemberShape) =
        httpBindingIndex.determineTimestampFormat(memberShape, HttpBinding.Location.PAYLOAD, defaultTimestampFormat)

    private fun serializerName(rustType: RustType, memberShape: MemberShape, suffix: String): String {
        val context = when {
            rustType.contains(instant) -> tsFormat(memberShape).name.replace('-', '_').toLowerCase()
            else -> null
        }
        val typeToFnName =
            rustType.stripOuter<RustType.Reference>().render(fullyQualified = true).filter { it.isLetterOrDigit() }
                .toLowerCase()
        return listOfNotNull(typeToFnName, context, suffix).joinToString("_")
    }

    private fun serializeFn(
        rustWriter: RustWriter,
        functionName: String,
        symbol: Symbol,
        body: RustWriter.() -> Unit
    ) {
        rustWriter.rustBlock(
            "pub fn $functionName<S>(_inp: #1T, _serializer: S) -> " +
                "Result<<S as #2T>::Ok, <S as #2T>::Error> where S: #2T",
            serializerType(symbol),
            RuntimeType.Serializer
        ) {
            body(this)
        }
    }

    private fun deserializeFn(
        rustWriter: RustWriter,
        functionName: String,
        symbol: Symbol,
        body: RustWriter.() -> Unit
    ) {
        rustWriter.rustBlock(
            "pub fn $functionName<'de, D>(_deser: D) -> Result<#T, D::Error> where D: #T<'de>",
            symbol,
            RuntimeType.Deserializer
        ) {
            body(this)
        }
    }

    fun serializerFor(memberShape: MemberShape): RuntimeType? {
        val symbol = symbolProvider.toSymbol(memberShape)
        val rustType = symbol.rustType()
        if (customShapes.none { rustType.contains(it) }) {
            return null
        }
        val fnName = serializerName(rustType, memberShape, "ser")
        return RuntimeType.forInlineFun(fnName, "serde_util") { writer ->
            serializeFn(writer, fnName, symbol) {
                handWrittenSerializers[fnName]?.also { it(this) } ?: write("todo!()")
            }
        }
    }

    fun deserializerFor(memberShape: MemberShape): RuntimeType? {
        val symbol = symbolProvider.toSymbol(memberShape)
        val rustType = symbol.rustType()
        if (customShapes.none { rustType.contains(it) }) {
            return null
        }
        val fnName = serializerName(rustType, memberShape, "deser")
        return RuntimeType.forInlineFun(fnName, "serde_util") { writer ->
            deserializeFn(writer, fnName, symbol) {
                if (rustType.contains(document)) {
                    write("todo!()")
                } else {
                    deserializer(rustType, memberShape)
                }
            }
        }
    }
}
