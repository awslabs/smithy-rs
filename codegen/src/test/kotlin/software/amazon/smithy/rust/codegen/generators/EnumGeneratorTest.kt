/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.generators

import org.junit.jupiter.api.Test
import software.amazon.smithy.codegen.core.SymbolProvider
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.traits.DocumentationTrait
import software.amazon.smithy.model.traits.EnumDefinition
import software.amazon.smithy.model.traits.EnumTrait
import software.amazon.smithy.rust.codegen.lang.RustWriter
import software.amazon.smithy.rust.codegen.smithy.SymbolVisitor
import software.amazon.smithy.rust.codegen.smithy.generators.EnumGenerator
import software.amazon.smithy.rust.testutil.asSmithy
import software.amazon.smithy.rust.testutil.quickTest
import software.amazon.smithy.rust.testutil.shouldCompile
import software.amazon.smithy.rust.testutil.shouldParseAsRust

class EnumGeneratorTest {
    @Test
    fun `it generates named enums`() {
        val trait = EnumTrait.builder()
            .addEnum(EnumDefinition.builder().value("t2.nano").name("T2_NANO").build())
            .addEnum(
                EnumDefinition.builder().value("t2.micro").name("T2_MICRO").documentation(
                            "T2 instances are Burstable Performance\n" +
                            "Instances that provide a baseline level of CPU\n" +
                            "performance with the ability to burst above the\n" +
                            "baseline."
                ).build()
            )
            .build()

        val shape = StringShape.builder()
            .id("com.test#InstanceType")
            .addTrait(trait)
            .addTrait(DocumentationTrait("Documentation for this enum"))
            .build()

        val model = Model.assembler()
            .addShapes(shape)
            .assemble()
            .unwrap()
        val provider: SymbolProvider = SymbolVisitor(model, "test")
        val writer = RustWriter.forModule("model")
        val generator = EnumGenerator(provider, writer, shape, trait)
        generator.render()
        val result = writer.toString()
        result.shouldParseAsRust()
        result.shouldCompile()
        result.quickTest("""
            let instance = InstanceType::T2Micro;
            assert_eq!(instance.as_str(), "t2.micro");
            assert_eq!(InstanceType::from("t2.nano"), InstanceType::T2Nano);
            assert_eq!(InstanceType::from("other"), InstanceType::Unknown("other".to_owned()));
            // round trip unknown variants:
            assert_eq!(InstanceType::from("other").as_str(), "other");
        """.trimIndent())
    }

    @Test
    fun `it generates unamed enums`() {
        val model = """
            namespace test
            @enum([
            {
                value: "Foo",
            },
            {
                value: "Baz",
            },
            {
                value: "Bar",
            },
            {
                value: "1",
            },
            {
                value: "0",
            },
        ])
        string FooEnum
        """.asSmithy()
        val shape = model.expectShape(ShapeId.from("test#FooEnum"), StringShape::class.java)
        val trait = shape.expectTrait(EnumTrait::class.java)
        val provider: SymbolProvider = SymbolVisitor(model, "test")
        val writer = RustWriter.forModule("model")
        val generator = EnumGenerator(provider, writer, shape, trait)
        generator.render()
        writer.shouldCompile("""
            // Values should be sorted
            assert_eq!(FooEnum::${EnumGenerator.Values}(), ["0", "1", "Bar", "Baz", "Foo"]);
        """)
    }
}
