package software.amazon.smithy.rust.codegen.server.smithy

import org.junit.jupiter.api.Test
import software.amazon.smithy.model.Model
import software.amazon.smithy.rust.codegen.core.rustlang.RustModule
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Visibility
import software.amazon.smithy.rust.codegen.core.rustlang.comment
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.smithy.InputsModule
import software.amazon.smithy.rust.codegen.core.smithy.ModelsModule
import software.amazon.smithy.rust.codegen.core.smithy.OutputsModule
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeCrateLocation
import software.amazon.smithy.rust.codegen.core.smithy.RustCrate
import software.amazon.smithy.rust.codegen.core.testutil.TestWorkspace
import software.amazon.smithy.rust.codegen.core.testutil.asSmithyModel
import software.amazon.smithy.rust.codegen.core.testutil.compileAndTest
import software.amazon.smithy.rust.codegen.core.testutil.generatePluginContext
import software.amazon.smithy.rust.codegen.core.testutil.unitTest
import software.amazon.smithy.rust.codegen.server.smithy.testutil.serverTestCodegenContext
import software.amazon.smithy.rust.codegen.server.smithy.testutil.serverTestSymbolProvider
import java.io.File
import kotlin.collections.Map.Entry

class RustCrateInlineModuleComposingWriterTest {
    private val rustCrate: RustCrate
    private val codegenContext: ServerCodegenContext
    private val model: Model = """
            ${'$'}version: "2.0"
            namespace test

            use aws.api#data
            use aws.protocols#restJson1

            @title("Weather Service")
            @restJson1
            service WeatherService {
                operations: [MalformedPatternOverride]
            }

            @suppress(["UnstableTrait"])
            @http(uri: "/MalformedPatternOverride", method: "GET")
            operation MalformedPatternOverride {
                output: MalformedPatternOverrideInput,
                errors: []
            }

            structure MalformedPatternOverrideInput {
                @pattern("^[g-m]+${'$'}")
                string: PatternString,
            }

            @pattern("^[a-m]+${'$'}")
            string PatternString
        """.trimIndent().asSmithyModel()

    init {
        codegenContext = serverTestCodegenContext(model)
        val runtimeConfig =
            RuntimeConfig(runtimeCrateLocation = RuntimeCrateLocation.Path(File("../rust-runtime").absolutePath))

        val (context, _) = generatePluginContext(
            model,
            runtimeConfig = runtimeConfig,
        )
        val settings = ServerRustSettings.from(context.model, context.settings)
        rustCrate = RustCrate(context.fileManifest, codegenContext.symbolProvider, settings.codegenConfig)
    }

    private fun createTestInlineModule(parentModule: RustModule, moduleName : String) : RustModule.LeafModule =
        RustModule.new(
            moduleName,
            visibility = Visibility.PUBLIC,
            documentation = moduleName,
            parent = parentModule,
            inline = true,
        )

    private fun createTestOrphanInlineModule(moduleName : String) : RustModule.LeafModule =
        RustModule.new(
            moduleName,
            visibility = Visibility.PUBLIC,
            documentation = moduleName,
            parent = RustModule.LibRs,
            inline = true,
        )

    private fun helloWorld(writer: RustWriter, moduleName: String) {
        writer.rustBlock("pub fn hello_world()") {
            writer.comment("Module $moduleName")
            writer.rust("""println!("from inside $moduleName");""")
        }
    }

    private fun byeWorld(writer: RustWriter, moduleName: String) {
        writer.rustBlock("pub fn bye_world()") {
            writer.comment("Module $moduleName")
            writer.rust("""println!("from inside $moduleName");""")
        }
    }

    @Test
    fun `simple inline module works`() {
        val testProject = TestWorkspace.testProject(serverTestSymbolProvider(model))
        val moduleA = createTestInlineModule(ModelsModule, "a")
        testProject.withModule(ModelsModule) {
            testProject.getInlineModuleWriter().withInlineModule(this, moduleA) {
                helloWorld(this, "a")
            }
        }

        testProject.getInlineModuleWriter().render()
        testProject.withModule(ModelsModule) {
            this.unitTest("test_a") {
                rust("crate::model::a::hello_world();")
            }
        }
        testProject.compileAndTest()
    }

    @Test
    fun `creating nested modules works from different rustWriters`() {
        // Define the following functions in different inline modules.
        // crate::model::a::hello_world();
        // crate::model::a::bye_world();
        // crate::model::b::hello_world();
        // crate::model::b::bye_world();
        // crate::model::b::c::hello_world();
        // crate::model::b::c::bye_world();
        // crate::input::e::hello_world();
        // crate::output::f::hello_world();
        // crate::output::f::g::hello_world();
        // crate::output::h::hello_world();
        // crate::output::h::i::hello_world();

        val testProject = TestWorkspace.testProject(serverTestSymbolProvider(model))
        val modules = hashMapOf(
            "a" to createTestOrphanInlineModule("a"),
            "d" to createTestOrphanInlineModule("d"),
            "e" to createTestOrphanInlineModule("e"),
            "i" to createTestOrphanInlineModule("i"),
        )

        modules["b"] = createTestInlineModule(ModelsModule, "b")
        modules["c"] = createTestInlineModule(modules["b"]!!, "c")
        modules["f"] = createTestInlineModule(OutputsModule, "f")
        modules["g"] = createTestInlineModule(modules["f"]!!, "g")
        modules["h"] = createTestInlineModule(OutputsModule, "h")

        testProject.withModule(ModelsModule) {
            testProject.getInlineModuleWriter().withInlineModule(this, modules["a"]!!) {
                helloWorld(this, "a")
            }
            testProject.getInlineModuleWriter().withInlineModule(this, modules["b"]!!) {
                helloWorld(this, "b")
                testProject.getInlineModuleWriter().withInlineModule(this, modules["c"]!!) {
                    byeWorld(this, "b::c")
                }
            }
            // Writing to the same module crate::model::a second time should work.
            testProject.getInlineModuleWriter().withInlineModule(this, modules["a"]!!) {
                byeWorld(this, "a")
            }
            // Writing to model::b, when model::b and model::b::c have already been written to
            // should work.
            testProject.getInlineModuleWriter().withInlineModule(this, modules["b"]!!) {
                byeWorld(this, "b")
            }
        }

        // Write directly to an inline module without specifying the immediate parent. crate::model::b::c
        // should have a `hello_world` fn in it now.
        testProject.withModule(ModelsModule) {
            testProject.getInlineModuleWriter().withInlineModuleHierarchy(this, modules["c"]!!) {
                helloWorld(this, "c")
            }
        }
        // Write to a different top level module to confirm that works.
        testProject.withModule(InputsModule) {
            testProject.getInlineModuleWriter().withInlineModuleHierarchy(this, modules["e"]!!) {
                helloWorld(this, "e")
            }
        }

        // Create a descendent inner module crate::output::f::g and then try writing to the intermediate inner module
        // that did not exist before the descendent was dded.
        testProject.getInlineModuleWriter().withInlineModuleHierarchyUsingCrate(testProject, modules["f"]!!) {
            testProject.getInlineModuleWriter().withInlineModuleHierarchyUsingCrate(testProject, modules["g"]!!) {
                helloWorld(this, "g")
            }
        }

        testProject.getInlineModuleWriter().withInlineModuleHierarchyUsingCrate(testProject, modules["f"]!!) {
            helloWorld(this, "f")
        }


        // It should work even if the inner descendent module was added using `withInlineModuleHierarchy` and then
        // code is added to the intermediate module using `withInlineModuleHierarchyUsingCrate`
        testProject.withModule(OutputsModule) {
            testProject.getInlineModuleWriter().withInlineModuleHierarchy(this, modules["h"]!!) {
                testProject.getInlineModuleWriter().withInlineModuleHierarchy(this, modules["i"]!!) {
                    helloWorld(this, "i")
                }
                testProject.withModule(ModelsModule) {
                    // While writing to output::h::i, it should be able to a completely different module
                    testProject.getInlineModuleWriter().withInlineModuleHierarchy(this, modules["b"]!!) {
                        rustBlock("pub fn some_other_writer_wrote_this()") {
                            rust("""println!("from inside crate::model::b::some_other_writer_wrote_this");""")
                        }
                    }
                }
            }
        }
        testProject.getInlineModuleWriter().withInlineModuleHierarchyUsingCrate(testProject, modules["h"]!!) {
            helloWorld(this, "h")
        }

        // Render all of the code.
        testProject.getInlineModuleWriter().render()

        testProject.withModule(ModelsModule) {
            this.unitTest("test_a") {
                rust("crate::model::a::hello_world();")
                rust("crate::model::a::bye_world();")
            }
            this.unitTest("test_b") {
                rust("crate::model::b::hello_world();")
                rust("crate::model::b::bye_world();")
            }
            this.unitTest("test_someother_writer_wrote") {
                rust("crate::model::b::some_other_writer_wrote_this();")
            }
            this.unitTest("test_b_c") {
                rust("crate::model::b::c::hello_world();")
                rust("crate::model::b::c::bye_world();")
            }
            this.unitTest("test_e") {
                rust("crate::input::e::hello_world();")
            }
            this.unitTest("test_f") {
                rust("crate::output::f::hello_world();")
            }
            this.unitTest("test_g") {
                rust("crate::output::f::g::hello_world();")
            }
            this.unitTest("test_h") {
                rust("crate::output::h::hello_world();")
            }
            this.unitTest("test_h_i") {
                rust("crate::output::h::i::hello_world();")
            }
        }
        testProject.compileAndTest()
    }
}
