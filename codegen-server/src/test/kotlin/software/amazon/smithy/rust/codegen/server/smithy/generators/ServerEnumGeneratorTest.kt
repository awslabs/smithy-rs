/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.generators

import io.kotest.matchers.string.shouldNotContain
import org.junit.jupiter.api.Test
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.server.smithy.ConstraintViolationSymbolProvider
import software.amazon.smithy.rust.codegen.server.smithy.testutil.serverTestCodegenContext
import software.amazon.smithy.rust.codegen.testutil.asSmithyModel
import software.amazon.smithy.rust.codegen.testutil.compileAndTest
import software.amazon.smithy.rust.codegen.util.expectTrait
import software.amazon.smithy.rust.codegen.util.lookup

class ServerEnumGeneratorTest {
    private val model = """
        namespace test
        @enum([
            {
                value: "t2.nano",
                name: "T2_NANO",
                documentation: "T2 instances are Burstable Performance Instances.",
                tags: ["ebsOnly"]
            },
            {
                value: "t2.micro",
                name: "T2_MICRO",
                documentation: "T2 instances are Burstable Performance Instances.",
                tags: ["ebsOnly"]
            },
        ])
        string InstanceType
    """.asSmithyModel()

    private val codegenContext = serverTestCodegenContext(model)
    private val symbolProvider = codegenContext.symbolProvider
    private val serviceShape = codegenContext.serviceShape
    private val constraintViolationSymbolProvider = ConstraintViolationSymbolProvider(symbolProvider, model, serviceShape)
    private val writer = RustWriter.forModule("model")
    private val shape = model.lookup<StringShape>("test#InstanceType")

    @Test
    fun `it generates TryFrom, FromStr and errors for enums`() {
        ServerEnumGenerator(
            model,
            symbolProvider,
            constraintViolationSymbolProvider,
            writer,
            shape,
            shape.expectTrait()
        ).render()
        // TODO Last line needs to be replaced with the `ConstraintViolation`.
        writer.compileAndTest(
            """
            use std::str::FromStr;
            assert_eq!(InstanceType::try_from("t2.nano").unwrap(), InstanceType::T2Nano);
            assert_eq!(InstanceType::from_str("t2.nano").unwrap(), InstanceType::T2Nano);
            //assert_eq!(InstanceType::try_from("unknown").unwrap_err(), InstanceTypeUnknownVariantError("unknown".to_string()));
            """
        )
    }

    @Test
    fun `it generates enums without the unknown variant`() {
        ServerEnumGenerator(
            model,
            symbolProvider,
            constraintViolationSymbolProvider,
            writer,
            shape,
            shape.expectTrait()
        ).render()
        writer.compileAndTest(
            """
            // check no unknown
            let instance = InstanceType::T2Micro;
            match instance {
                InstanceType::T2Micro => (),
                InstanceType::T2Nano => (),
            }
            """
        )
    }

    @Test
    fun `it generates enums without non_exhaustive`() {
        ServerEnumGenerator(
            model,
            symbolProvider,
            constraintViolationSymbolProvider,
            writer,
            shape,
            shape.expectTrait()
        ).render()
        writer.toString() shouldNotContain "#[non_exhaustive]"
    }
}
