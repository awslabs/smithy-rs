/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.server.smithy

import java.util.logging.Logger
import software.amazon.smithy.build.PluginContext
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.neighbor.Walker
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ServiceShape
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.model.shapes.ShapeVisitor
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.traits.EnumTrait
import software.amazon.smithy.rust.codegen.rustlang.RustMetadata
import software.amazon.smithy.rust.codegen.rustlang.RustModule
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.smithy.DefaultPublicModules
import software.amazon.smithy.rust.codegen.smithy.RustCrate
import software.amazon.smithy.rust.codegen.smithy.RustSettings
import software.amazon.smithy.rust.codegen.smithy.RustSymbolProvider
import software.amazon.smithy.rust.codegen.smithy.SymbolVisitorConfig
import software.amazon.smithy.rust.codegen.smithy.customize.RustCodegenDecorator
import software.amazon.smithy.rust.codegen.smithy.generators.BuilderGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.EnumGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.HttpProtocolGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolConfig
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolGeneratorFactory
import software.amazon.smithy.rust.codegen.smithy.generators.ServiceGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.StructureGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.UnionGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.implBlock
import software.amazon.smithy.rust.codegen.smithy.protocols.HttpTraitHttpBindingResolver
import software.amazon.smithy.rust.codegen.smithy.protocols.ProtocolContentTypes
import software.amazon.smithy.rust.codegen.smithy.protocols.ProtocolLoader
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticInputTrait
import software.amazon.smithy.rust.codegen.smithy.transformers.AddErrorMessage
import software.amazon.smithy.rust.codegen.smithy.transformers.EventStreamNormalizer
import software.amazon.smithy.rust.codegen.smithy.transformers.OperationNormalizer
import software.amazon.smithy.rust.codegen.smithy.transformers.RecursiveShapeBoxer
import software.amazon.smithy.rust.codegen.util.CommandFailed
import software.amazon.smithy.rust.codegen.util.getTrait
import software.amazon.smithy.rust.codegen.util.hasTrait
import software.amazon.smithy.rust.codegen.util.runCommand

class CodegenVisitor(context: PluginContext, private val codegenDecorator: RustCodegenDecorator) :
        ShapeVisitor.Default<Unit>() {

    private val logger = Logger.getLogger(javaClass.name)
    private val settings = RustSettings.from(context.model, context.settings)

    private val symbolProvider: RustSymbolProvider
    private val rustCrate: RustCrate
    private val fileManifest = context.fileManifest
    private val model: Model
    private val protocolConfig: ProtocolConfig
    private val protocolGenerator: ProtocolGeneratorFactory<HttpProtocolGenerator>
    private val httpGenerator: HttpProtocolGenerator

    private val serializerGenerator: RestJsonServerSerializerGenerator
    private val deserializerGenerator: RestJsonDeserializerGenerator
    private val httpSerializerGenerator: HttpSerializerGenerator
    private val httpDeserializerGenerator: HttpDeserializerGenerator

    init {
        val symbolVisitorConfig =
                SymbolVisitorConfig(
                        runtimeConfig = settings.runtimeConfig,
                        codegenConfig = settings.codegenConfig
                )
        val baseModel = baselineTransform(context.model)
        val service = settings.getService(baseModel)
        val (protocol, generator) =
                ProtocolLoader(
                                codegenDecorator.protocols(
                                        service.id,
                                        ProtocolLoader.DefaultProtocols
                                )
                        )
                        .protocolFor(context.model, service)
        protocolGenerator = generator
        model = generator.transformModel(codegenDecorator.transformModel(service, baseModel))
        val baseProvider = RustCodegenPlugin.baseSymbolProvider(model, service, symbolVisitorConfig)
        symbolProvider =
                codegenDecorator.symbolProvider(generator.symbolProvider(model, baseProvider))

        protocolConfig =
                ProtocolConfig(
                        model,
                        symbolProvider,
                        settings.runtimeConfig,
                        service,
                        protocol,
                        settings.moduleName
                )
        rustCrate = RustCrate(context.fileManifest, symbolProvider, DefaultPublicModules)
        httpGenerator = protocolGenerator.buildProtocolGenerator(protocolConfig)

        val httpBindingResolver =
                HttpTraitHttpBindingResolver(
                        protocolConfig.model,
                        ProtocolContentTypes.consistent("application/json"),
                )
        serializerGenerator = RestJsonServerSerializerGenerator(protocolConfig, httpBindingResolver)
        deserializerGenerator = RestJsonDeserializerGenerator(protocolConfig, httpBindingResolver)
        httpSerializerGenerator = HttpSerializerGenerator(protocolConfig, httpBindingResolver)
        httpDeserializerGenerator = HttpDeserializerGenerator(protocolConfig, httpBindingResolver)
    }

    private fun baselineTransform(model: Model) =
            model.let(RecursiveShapeBoxer::transform)
                    .let(AddErrorMessage::transform)
                    .let(OperationNormalizer::transform)
                    .let(EventStreamNormalizer::transform)

    fun execute() {
        logger.info("generating Rust server...")
        val service = settings.getService(model)
        val serviceShapes = Walker(model).walkShapes(service)
        logger.info("${serviceShapes}")
        serviceShapes.forEach { it.accept(this) }
        codegenDecorator.extras(protocolConfig, rustCrate)
        rustCrate.finalize(settings, codegenDecorator.libRsCustomizations(protocolConfig, listOf()))
        try {
            "cargo fmt".runCommand(
                    fileManifest.baseDir,
                    timeout = settings.codegenConfig.formatTimeoutSeconds.toLong()
            )
        } catch (err: CommandFailed) {
            logger.warning("Failed to run cargo fmt: [${service.id}]\n${err.output}")
        }

        logger.info("Rust server generation complete!")
    }

    override fun getDefault(shape: Shape?) {}

    override fun operationShape(shape: OperationShape?) {
        logger.fine("generating an operation...")
        val module = RustMetadata(public = true)
        rustCrate.withModule(RustModule("json_serde", module)) { writer ->
            renderSerdeError(writer)
            shape?.let {
                httpDeserializerGenerator.render(writer, it)
                httpSerializerGenerator.render(writer, it)
                serializerGenerator.render(writer, it)
                deserializerGenerator.render(writer, it)
            }
        }
    }

    override fun structureShape(shape: StructureShape) {
        logger.fine("generating a structure...")
        rustCrate.useShapeWriter(shape) { writer ->
            StructureGenerator(model, symbolProvider, writer, shape).render()
            if (!shape.hasTrait<SyntheticInputTrait>()) {
                val builderGenerator =
                        BuilderGenerator(protocolConfig.model, protocolConfig.symbolProvider, shape)
                builderGenerator.render(writer)
                writer.implBlock(shape, symbolProvider) {
                    builderGenerator.renderConvenienceMethod(this)
                }
            }
        }
    }

    override fun stringShape(shape: StringShape) {
        shape.getTrait<EnumTrait>()?.also { enum ->
            rustCrate.useShapeWriter(shape) { writer ->
                EnumGenerator(model, symbolProvider, writer, shape, enum).render()
            }
        }
    }

    override fun unionShape(shape: UnionShape) {
        rustCrate.useShapeWriter(shape) {
            UnionGenerator(model, symbolProvider, it, shape).render()
        }
    }

    override fun serviceShape(shape: ServiceShape) {
        ServiceGenerator(
                        rustCrate,
                        httpGenerator,
                        protocolGenerator.support(),
                        protocolConfig,
                        codegenDecorator
                )
                .render()
    }

    private fun renderSerdeError(writer: RustWriter) {
        writer.rust(
                """
                ##[derive(Debug)]
                pub enum Error {
                    Generic(std::borrow::Cow<'static, str>),
                    DeserializeJson(smithy_json::deserialize::Error),
                    DeserializeHeader(smithy_http::header::ParseError),
                    DeserializeLabel(std::string::String),
                    BuildInput(smithy_http::operation::BuildError),
                    BuildResponse(http::Error),
                }
                
                impl Error {
                    ##[allow(dead_code)]
                    fn generic(msg: &'static str) -> Self {
                        Self::Generic(msg.into())
                    }
                }
                
                impl From<smithy_json::deserialize::Error> for Error {
                    fn from(err: smithy_json::deserialize::Error) -> Self {
                        Self::DeserializeJson(err)
                    }
                }
                
                impl From<smithy_http::header::ParseError> for Error {
                    fn from(err: smithy_http::header::ParseError) -> Self {
                        Self::DeserializeHeader(err)
                    }
                }
                
                impl From<smithy_http::operation::BuildError> for Error {
                    fn from(err: smithy_http::operation::BuildError) -> Self {
                        Self::BuildInput(err)
                    }
                }
                                
                impl std::fmt::Display for Error {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        match *self {
                            Self::Generic(ref msg) => write!(f, "serde error: {}", msg),
                            Self::DeserializeJson(ref err) => write!(f, "json parse error: {}", err),
                            Self::DeserializeHeader(ref err) => write!(f, "header parse error: {}", err),
                            Self::DeserializeLabel(ref msg) => write!(f, "label parse error: {}", msg),
                            Self::BuildInput(ref err) => write!(f, "json payload error: {}", err),
                            Self::BuildResponse(ref err) => write!(f, "http response error: {}", err),
                        }
                    }
                }
                
                impl std::error::Error for Error {}
            """.trimIndent()
        )
    }
}
