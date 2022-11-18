/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import io.kotest.matchers.string.shouldContain
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.extension.ExtensionContext
import org.junit.jupiter.params.ParameterizedTest
import org.junit.jupiter.params.provider.Arguments
import org.junit.jupiter.params.provider.ArgumentsProvider
import org.junit.jupiter.params.provider.ArgumentsSource
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.traits.LengthTrait
import software.amazon.smithy.model.traits.PatternTrait
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.smithy.ModelsModule
import software.amazon.smithy.rust.codegen.core.testutil.TestWorkspace
import software.amazon.smithy.rust.codegen.core.testutil.asSmithyModel
import software.amazon.smithy.rust.codegen.core.testutil.compileAndTest
import software.amazon.smithy.rust.codegen.core.testutil.unitTest
import software.amazon.smithy.rust.codegen.core.util.expectTrait
import software.amazon.smithy.rust.codegen.core.util.lookup
import software.amazon.smithy.rust.codegen.server.smithy.testutil.serverTestCodegenContext
import java.util.stream.Stream

class ConstrainedStringGeneratorTest {
    val a = PatternTrait("something")
    data class TestCase(val model: Model, val validString: String, val invalidString: String)

    class ConstrainedStringGeneratorTestProvider : ArgumentsProvider {
        private val lengthTestCases = listOf(
            // Min and max.
            Triple("@length(min: 11, max: 12)", "validString", "invalidString"),
            // Min equal to max.
            Triple("@length(min: 11, max: 11)", "validString", "invalidString"),
            // Only min.
            Triple("@length(min: 11)", "validString", ""),
            // Only max.
            Triple("@length(max: 11)", "", "invalidString"),
            // Count Unicode scalar values, not `.len()`.
            Triple(
                "@length(min: 3, max: 3)",
                "👍👍👍", // These three emojis are three Unicode scalar values.
                "👍👍👍👍",
            ),
            // Need to fix the setup to be able to add `@pattern` tests.
            // Triple("@pattern(\"^[a-z]+$\")", "valid", "123 invalid"),
        ).map {
            TestCase(
                """
                namespace test

                ${it.first}
                string ConstrainedString
                """.asSmithyModel(),
                it.second,
                it.third,
            )
        }

        override fun provideArguments(context: ExtensionContext?): Stream<out Arguments> =
            lengthTestCases.map { Arguments.of(it) }.stream()
    }

    @ParameterizedTest
    @ArgumentsSource(ConstrainedStringGeneratorTestProvider::class)
    fun `it should generate constrained string types`(testCase: TestCase) {
        val constrainedStringShape = testCase.model.lookup<StringShape>("test#ConstrainedString")

        val codegenContext = serverTestCodegenContext(testCase.model)
        val symbolProvider = codegenContext.symbolProvider

        val project = TestWorkspace.testProject(symbolProvider)

        project.withModule(ModelsModule) {
            ConstrainedStringGenerator(codegenContext, this, constrainedStringShape, listOf(constrainedStringShape.expectTrait<LengthTrait>())).render()

            unitTest(
                name = "try_from_success",
                test = """
                    let string = "${testCase.validString}".to_owned();
                    let _constrained: ConstrainedString = string.try_into().unwrap();
                """,
            )
            unitTest(
                name = "try_from_fail",
                test = """
                    let string = "${testCase.invalidString}".to_owned();
                    let constrained_res: Result<ConstrainedString, _> = string.try_into();
                    constrained_res.unwrap_err();
                """,
            )
            unitTest(
                name = "inner",
                test = """
                    let string = "${testCase.validString}".to_owned();
                    let constrained = ConstrainedString::try_from(string).unwrap();

                    assert_eq!(constrained.inner(), "${testCase.validString}");
                """,
            )
            unitTest(
                name = "into_inner",
                test = """
                    let string = "${testCase.validString}".to_owned();
                    let constrained = ConstrainedString::try_from(string.clone()).unwrap();

                    assert_eq!(constrained.into_inner(), string);
                """,
            )
        }

        project.compileAndTest()
    }

    @Test
    fun `type should not be constructible without using a constructor`() {
        val model = """
            namespace test

            @length(min: 1, max: 69)
            string ConstrainedString
        """.asSmithyModel()
        val constrainedStringShape = model.lookup<StringShape>("test#ConstrainedString")

        val codegenContext = serverTestCodegenContext(model)

        val writer = RustWriter.forModule(ModelsModule.name)

        ConstrainedStringGenerator(codegenContext, writer, constrainedStringShape, listOf(constrainedStringShape.expectTrait<LengthTrait>())).render()

        // Check that the wrapped type is `pub(crate)`.
        writer.toString() shouldContain "pub struct ConstrainedString(pub(crate) std::string::String);"
    }

    @Test
    fun `Display implementation`() {
        val model = """
            namespace test

            @length(min: 1, max: 69)
            string ConstrainedString

            @sensitive
            @length(min: 1, max: 78)
            string SensitiveConstrainedString
        """.asSmithyModel()
        val constrainedStringShape = model.lookup<StringShape>("test#ConstrainedString")
        val sensitiveConstrainedStringShape = model.lookup<StringShape>("test#SensitiveConstrainedString")

        val codegenContext = serverTestCodegenContext(model)

        val project = TestWorkspace.testProject(codegenContext.symbolProvider)

        project.withModule(ModelsModule) {
            ConstrainedStringGenerator(codegenContext, this, constrainedStringShape, listOf(constrainedStringShape.expectTrait<LengthTrait>())).render()
            ConstrainedStringGenerator(codegenContext, this, sensitiveConstrainedStringShape, listOf(sensitiveConstrainedStringShape.expectTrait<LengthTrait>())).render()

            unitTest(
                name = "non_sensitive_string_display_implementation",
                test = """
                    let string = "a non-sensitive string".to_owned();
                    let constrained = ConstrainedString::try_from(string).unwrap();
                    assert_eq!(format!("{}", constrained), "a non-sensitive string")
                """,
            )

            unitTest(
                name = "sensitive_string_display_implementation",
                test = """
                    let string = "That is how heavy a secret can become. It can make blood flow easier than ink.".to_owned();
                    let constrained = SensitiveConstrainedString::try_from(string).unwrap();
                    assert_eq!(format!("{}", constrained), "*** Sensitive Data Redacted ***")
                """,
            )
        }

        project.compileAndTest()
    }
}
