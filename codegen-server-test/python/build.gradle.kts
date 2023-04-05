/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

description = "Generates Rust/Python code from Smithy models and runs the protocol tests"
extra["displayName"] = "Smithy :: Rust :: Codegen :: Server :: Python :: Test"
extra["moduleName"] = "software.amazon.smithy.rust.kotlin.codegen.server.python.test"

tasks["jar"].enabled = false

plugins {
    id("software.amazon.smithy")
}

val smithyVersion: String by project
val defaultRustDocFlags: String by project
val properties = PropertyRetriever(rootProject, project)

val pluginName = "rust-server-codegen-python"
val workingDirUnderBuildDir = "smithyprojections/codegen-server-test-python/"

configure<software.amazon.smithy.gradle.SmithyExtension> {
    outputDirectory = file("$buildDir/$workingDirUnderBuildDir")
}

buildscript {
    val smithyVersion: String by project
    dependencies {
        classpath("software.amazon.smithy:smithy-cli:$smithyVersion")
    }
}

dependencies {
    implementation(project(":codegen-server:python"))
    implementation("software.amazon.smithy:smithy-aws-protocol-tests:$smithyVersion")
    implementation("software.amazon.smithy:smithy-protocol-test-traits:$smithyVersion")
    implementation("software.amazon.smithy:smithy-aws-traits:$smithyVersion")
}

val allCodegenTests = "../../codegen-core/common-test-models".let { commonModels ->
    listOf(
        CodegenTest("com.amazonaws.simple#SimpleService", "simple", imports = listOf("$commonModels/simple.smithy")),
        CodegenTest("com.aws.example.python#PokemonService", "pokemon-service-server-sdk"),
        CodegenTest(
            "com.amazonaws.ebs#Ebs", "ebs",
            imports = listOf("$commonModels/ebs.json"),
            extraConfig = """, "codegen": { "ignoreUnsupportedConstraints": true } """,
        ),
        CodegenTest(
            "aws.protocoltests.misc#MiscService",
            "misc",
            imports = listOf("$commonModels/misc.smithy"),
            // TODO(https://github.com/awslabs/smithy-rs/issues/1401) `@uniqueItems` is used.
            extraConfig = """, "codegen": { "ignoreUnsupportedConstraints": true } """,
        ),
        CodegenTest(
            "aws.protocoltests.json#JsonProtocol",
            "json_rpc11",
            extraConfig = """, "codegen": { "ignoreUnsupportedConstraints": true } """,
        ),
        CodegenTest("aws.protocoltests.json10#JsonRpc10", "json_rpc10"),
        CodegenTest("aws.protocoltests.restjson#RestJson", "rest_json"),
        CodegenTest(
            "aws.protocoltests.restjson#RestJsonExtras",
            "rest_json_extras",
            imports = listOf("$commonModels/rest-json-extras.smithy"),
        ),
        // Fails with
        // java.lang.IllegalStateException: The two inline modules have the same name but different attributes on them.
        // CodegenTest(
        //     "aws.protocoltests.restjson.validation#RestJsonValidation",
        //     "rest_json_validation",
        //     // `@range` trait is used on floating point shapes, which we deliberately don't want to support.
        //     // See https://github.com/awslabs/smithy-rs/issues/1401.
        //     extraConfig = """, "codegen": { "ignoreUnsupportedConstraints": true } """,
        // ),
        // Fails because we need to customize the constraint traits to allow converting from Python
        // wrappers to their inner type.
        CodegenTest(
            "com.amazonaws.constraints#ConstraintsService",
            "constraints",
            imports = listOf("$commonModels/constraints.smithy"),
        ),
    )
}

project.registerGenerateSmithyBuildTask(rootProject, pluginName, allCodegenTests)
project.registerGenerateCargoWorkspaceTask(rootProject, pluginName, allCodegenTests, workingDirUnderBuildDir)
project.registerGenerateCargoConfigTomlTask(buildDir.resolve(workingDirUnderBuildDir))

tasks["smithyBuildJar"].dependsOn("generateSmithyBuild")
tasks["assemble"].finalizedBy("generateCargoWorkspace")

project.registerModifyMtimeTask()
project.registerCargoCommandsTasks(buildDir.resolve(workingDirUnderBuildDir), defaultRustDocFlags)

tasks["test"].finalizedBy(cargoCommands(properties).map { it.toString })

tasks["clean"].doFirst { delete("smithy-build.json") }
