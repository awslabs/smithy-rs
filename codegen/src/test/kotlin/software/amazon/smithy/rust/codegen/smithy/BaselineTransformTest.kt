/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.smithy

import io.kotest.matchers.shouldBe
import org.junit.jupiter.api.Test
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.traits.MixinTrait
import software.amazon.smithy.rust.codegen.testutil.asSmithyModel
import software.amazon.smithy.rust.codegen.util.cloneOperation
import software.amazon.smithy.rust.codegen.util.orNull
import software.amazon.smithy.rust.codegen.util.rename
import software.amazon.smithy.rust.codegen.testutil.generatePluginContext
import software.amazon.smithy.rust.codegen.smithy.customize.CombinedCodegenDecorator
import software.amazon.smithy.rust.codegen.smithy.customize.RequiredCustomizations
import software.amazon.smithy.rust.codegen.smithy.customize.NoOpEventStreamSigningDecorator
import software.amazon.smithy.rust.codegen.smithy.customizations.ClientCustomizations
import software.amazon.smithy.rust.codegen.smithy.generators.client.FluentClientDecorator
import software.amazon.smithy.rust.codegen.smithy.CodegenVisitor

fun String.asSmithyModel_Version2(sourceLocation: String? = null): Model {
    val processed = letIf(!this.startsWith("\$version")) { "\$version: \"2.0\"\n$it" }
    return Model.assembler().discoverModels().addUnparsedModel(sourceLocation ?: "test.smithy", processed).assemble()
        .unwrap()
}

class BaselineTransformTest {
    @Test
    fun `verify mixins removed`() {
        val model = """
            namespace com.example

            use aws.protocols#awsJson1_0

            @awsJson1_0
            @aws.api#service(sdkId: "Test", endpointPrefix: "differentPrefix")
            service Example {
                operations: [ BasicOperation ]
            }

            operation BasicOperation {
                input: Shape
            }

            @mixin
            structure SimpleMixin {
                name: String
            }

            structure Shape with [
                SimpleMixin
            ] {
                greeting: String
            }
        """.asSmithyModel_Version2()
        val (ctx, _) = generatePluginContext(model)
        val codegenDecorator =
            CombinedCodegenDecorator.fromClasspath(
                ctx,
                ClientCustomizations(),
                RequiredCustomizations(),
                FluentClientDecorator(),
                NoOpEventStreamSigningDecorator(),
            )
        val visitor = CodegenVisitor(ctx, codegenDecorator)
        val baselineModel = visitor.baselineTransform(model)
        baselineModel.getShapesWithTrait(ShapeId.from("smithy.api#mixin")).isEmpty() shouldBe true
    }
}
