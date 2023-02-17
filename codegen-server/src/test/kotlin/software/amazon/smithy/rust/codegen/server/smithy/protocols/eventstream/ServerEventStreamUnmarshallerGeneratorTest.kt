/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.protocols.eventstream

import org.junit.jupiter.params.ParameterizedTest
import org.junit.jupiter.params.provider.ArgumentsSource
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.implBlock
import software.amazon.smithy.rust.codegen.core.smithy.CodegenTarget
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.smithy.RustCrate
import software.amazon.smithy.rust.codegen.core.smithy.generators.BuilderGenerator
import software.amazon.smithy.rust.codegen.core.smithy.protocols.Protocol
import software.amazon.smithy.rust.codegen.core.smithy.protocols.parse.EventStreamUnmarshallerGenerator
import software.amazon.smithy.rust.codegen.core.testutil.EventStreamTestTools
import software.amazon.smithy.rust.codegen.core.testutil.EventStreamTestVariety
import software.amazon.smithy.rust.codegen.core.testutil.TestEventStreamProject
import software.amazon.smithy.rust.codegen.core.testutil.compileAndTest
import software.amazon.smithy.rust.codegen.server.smithy.ServerCodegenContext
import software.amazon.smithy.rust.codegen.server.smithy.transformers.ConstrainedMemberTransform

class ServerEventStreamUnmarshallerGeneratorTest {
    @ParameterizedTest
    @ArgumentsSource(TestCasesProvider::class)
    fun test(testCase: TestCase) {
        // TODO(https://github.com/awslabs/smithy-rs/issues/1442): Enable tests for `publicConstrainedTypes = false`
        // by deleting this if/return
        if (!testCase.publicConstrainedTypes) {
            return
        }

        val testProject = EventStreamTestTools.setupTestCase(
            testCase.eventStreamTestCase,
            object : ServerEventStreamBaseRequirements() {
                override val publicConstrainedTypes: Boolean get() = testCase.publicConstrainedTypes

                override fun renderGenerator(
                    codegenContext: ServerCodegenContext,
                    project: TestEventStreamProject,
                    protocol: Protocol,
                ): RuntimeType {
                    return EventStreamUnmarshallerGenerator(
                        protocol,
                        codegenContext,
                        project.operationShape,
                        project.streamShape,
                    ).render()
                }

                // TODO(https://github.com/awslabs/smithy-rs/issues/1442): Delete this function override to use the correct builder from the parent class
                override fun renderBuilderForShape(
                    rustCrate: RustCrate,
                    writer: RustWriter,
                    codegenContext: ServerCodegenContext,
                    shape: StructureShape,
                ) {
                    val builderGen = BuilderGenerator(codegenContext.model, codegenContext.symbolProvider, shape, emptyList())
                    rustCrate.withModule(codegenContext.symbolProvider.moduleForBuilder(shape)) {
                        builderGen.render(this)
                    }
                    rustCrate.moduleFor(shape) {
                        writer.implBlock(codegenContext.symbolProvider.toSymbol(shape)) {
                            builderGen.renderConvenienceMethod(this)
                        }
                    }
                }
            },
            CodegenTarget.SERVER,
            EventStreamTestVariety.Unmarshall,
            transformers = listOf(ConstrainedMemberTransform::transform),
        )
        testProject.compileAndTest()
    }
}
