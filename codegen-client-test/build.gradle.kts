/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

extra["displayName"] = "Smithy :: Rust :: Codegen :: Test"
extra["moduleName"] = "software.amazon.smithy.kotlin.codegen.test"

tasks["jar"].enabled = false

plugins {
    id("software.amazon.smithy")
}

val smithyVersion: String by project
val defaultRustDocFlags: String by project
val properties = PropertyRetriever(rootProject, project)
fun getSmithyRuntimeMode(): String = properties.get("smithy.runtime.mode") ?: "orchestrator"

val pluginName = "rust-client-codegen"
val workingDirUnderBuildDir = "smithyprojections/codegen-client-test/"

buildscript {
    val smithyVersion: String by project
    dependencies {
        classpath("software.amazon.smithy:smithy-cli:$smithyVersion")
    }
}

dependencies {
    implementation(project(":codegen-client"))
    implementation("software.amazon.smithy:smithy-aws-protocol-tests:$smithyVersion")
    implementation("software.amazon.smithy:smithy-protocol-test-traits:$smithyVersion")
    implementation("software.amazon.smithy:smithy-aws-traits:$smithyVersion")
}

data class ClientTest(
    val serviceShapeName: String,
    val moduleName: String,
    val dependsOn: List<String> = emptyList(),
    val addMessageToErrors: Boolean = true,
    val renameErrors: Boolean = true,
) {
    fun toCodegenTest(): CodegenTest = CodegenTest(
        serviceShapeName,
        moduleName,
        extraCodegenConfig = extraCodegenConfig(),
        imports = imports(),
    )

    private fun extraCodegenConfig(): String = StringBuilder().apply {
        append("\"addMessageToErrors\": $addMessageToErrors,\n")
        append("\"renameErrors\": $renameErrors\n,")
        append("\"enableNewSmithyRuntime\": \"${getSmithyRuntimeMode()}\"")
    }.toString()

    private fun imports(): List<String> = dependsOn.map { "../codegen-core/common-test-models/$it" }
}

val allCodegenTests = listOf(
    ClientTest("com.amazonaws.simple#SimpleService", "simple", dependsOn = listOf("simple.smithy")),
    ClientTest("com.amazonaws.dynamodb#DynamoDB_20120810", "dynamo"),
    ClientTest("com.amazonaws.ebs#Ebs", "ebs", dependsOn = listOf("ebs.json")),
    ClientTest("aws.protocoltests.json10#JsonRpc10", "json_rpc10"),
    ClientTest("aws.protocoltests.json#JsonProtocol", "json_rpc11"),
    ClientTest("aws.protocoltests.restjson#RestJson", "rest_json"),
    ClientTest(
        "aws.protocoltests.restjson#RestJsonExtras",
        "rest_json_extras",
        dependsOn = listOf("rest-json-extras.smithy"),
    ),
    ClientTest("aws.protocoltests.misc#MiscService", "misc", dependsOn = listOf("misc.smithy")),
    ClientTest("aws.protocoltests.restxml#RestXml", "rest_xml", addMessageToErrors = false),
    ClientTest("aws.protocoltests.query#AwsQuery", "aws_query", addMessageToErrors = false),
    ClientTest("aws.protocoltests.ec2#AwsEc2", "ec2_query", addMessageToErrors = false),
    ClientTest("aws.protocoltests.restxml.xmlns#RestXmlWithNamespace", "rest_xml_namespace", addMessageToErrors = false),
    ClientTest("aws.protocoltests.restxml#RestXmlExtras", "rest_xml_extras", addMessageToErrors = false),
    ClientTest(
        "aws.protocoltests.restxmlunwrapped#RestXmlExtrasUnwrappedErrors",
        "rest_xml_extras_unwrapped",
        addMessageToErrors = false,
    ),
    ClientTest(
        "crate#Config",
        "naming_test_ops",
        dependsOn = listOf("naming-obstacle-course-ops.smithy"),
        renameErrors = false,
    ),
    ClientTest(
        "casing#ACRONYMInside_Service",
        "naming_test_casing",
        dependsOn = listOf("naming-obstacle-course-casing.smithy"),
    ),
    ClientTest(
        "naming_obs_structs#NamingObstacleCourseStructs",
        "naming_test_structs",
        dependsOn = listOf("naming-obstacle-course-structs.smithy"),
        renameErrors = false,
    ),
    ClientTest("aws.protocoltests.json#TestService", "endpoint-rules"),
    ClientTest(
        "com.aws.example#PokemonService",
        "pokemon-service-client",
        dependsOn = listOf("pokemon.smithy", "pokemon-common.smithy"),
    ),
    ClientTest(
        "com.aws.example#PokemonService",
        "pokemon-service-awsjson-client",
        dependsOn = listOf("pokemon-awsjson.smithy", "pokemon-common.smithy"),
    ),
    ClientTest(
        "com.amazonaws.simple#RpcV2Service",
        "rpcv2-pokemon-client",
        dependsOn = listOf("rpcv2.smithy")
    ),
).map(ClientTest::toCodegenTest)

project.registerGenerateSmithyBuildTask(rootProject, pluginName, allCodegenTests)
project.registerGenerateCargoWorkspaceTask(rootProject, pluginName, allCodegenTests, workingDirUnderBuildDir)
project.registerGenerateCargoConfigTomlTask(buildDir.resolve(workingDirUnderBuildDir))

tasks["generateSmithyBuild"].inputs.property("smithy.runtime.mode", getSmithyRuntimeMode())

tasks["smithyBuildJar"].dependsOn("generateSmithyBuild")
tasks["assemble"].finalizedBy("generateCargoWorkspace")

project.registerModifyMtimeTask()
project.registerCargoCommandsTasks(buildDir.resolve(workingDirUnderBuildDir), defaultRustDocFlags)

tasks["test"].finalizedBy(cargoCommands(properties).map { it.toString })

tasks["clean"].doFirst { delete("smithy-build.json") }
