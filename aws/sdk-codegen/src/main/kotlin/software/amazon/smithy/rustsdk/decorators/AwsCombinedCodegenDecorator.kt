/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rustsdk.decorators

import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.customizations.DocsRsMetadataDecorator
import software.amazon.smithy.rust.codegen.client.smithy.customizations.DocsRsMetadataSettings
import software.amazon.smithy.rust.codegen.client.smithy.customize.CombinedCodegenDecorator
import software.amazon.smithy.rust.codegen.client.smithy.customize.RustCodegenDecorator
import software.amazon.smithy.rust.codegen.client.smithy.generators.protocol.ClientProtocolGenerator
import software.amazon.smithy.rust.codegen.core.rustlang.RustModule
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustBlockTemplate
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.core.smithy.RustCrate
import software.amazon.smithy.rust.codegen.core.smithy.customize.writeCustomizations
import software.amazon.smithy.rustsdk.AwsCustomization
import software.amazon.smithy.rustsdk.AwsSection
import software.amazon.smithy.rustsdk.awsTypes
import software.amazon.smithy.rustsdk.servicedecorators.apigateway.ApiGatewayDecorator
import software.amazon.smithy.rustsdk.servicedecorators.auth.DisabledAuthDecorator
import software.amazon.smithy.rustsdk.servicedecorators.ec2.Ec2Decorator
import software.amazon.smithy.rustsdk.servicedecorators.glacier.GlacierDecorator
import software.amazon.smithy.rustsdk.servicedecorators.route53.Route53Decorator
import software.amazon.smithy.rustsdk.servicedecorators.s3.S3Decorator
import software.amazon.smithy.rustsdk.servicedecorators.sts.STSDecorator

val DECORATORS = listOf(
    // General AWS Decorators
    CredentialsProviderDecorator(),
    RegionDecorator(),
    AwsEndpointDecorator(),
    UserAgentDecorator(),
    SigV4SigningDecorator(),
    HttpRequestChecksumDecorator(),
    HttpResponseChecksumDecorator(),
    ResiliencyDecorator(),
    IntegrationTestDecorator(),
    AwsFluentClientDecorator(),
    CrateLicenseDecorator(),
    SdkConfigDecorator(),
    ServiceConfigDecorator(),
    AwsPresigningDecorator(),
    AwsReadmeDecorator(),

    // Service specific decorators
    ApiGatewayDecorator(),
    DisabledAuthDecorator(),
    Ec2Decorator(),
    GlacierDecorator(),
    Route53Decorator(),
    S3Decorator(),
    STSDecorator(),

    // Only build docs-rs for linux to reduce load on docs.rs
    DocsRsMetadataDecorator(DocsRsMetadataSettings(targets = listOf("x86_64-unknown-linux-gnu"), allFeatures = true)),
)

interface AwsCodegenDecorator : RustCodegenDecorator<ClientProtocolGenerator, ClientCodegenContext> {
    fun awsCustomizations(
        codegenContext: ClientCodegenContext,
        baseCustomizations: List<AwsCustomization>,
    ): List<AwsCustomization> {
        return baseCustomizations
    }
}

class AwsCombinedCodegenDecorator :
    CombinedCodegenDecorator<ClientProtocolGenerator, ClientCodegenContext>(DECORATORS) {
    override val order: Byte = -1

    override fun extras(codegenContext: ClientCodegenContext, rustCrate: RustCrate) {
        super.extras(codegenContext, rustCrate)

        val awsCustomizations = orderedDecorators.filterIsInstance<AwsCodegenDecorator>()
            .foldRight(listOf<AwsCustomization>()) { decorator, customizations ->
                decorator.awsCustomizations(codegenContext, customizations)
            }
        generateImplFromRefSdkConfigForConfigBuilder(codegenContext.runtimeConfig, rustCrate, awsCustomizations)
    }
}

fun generateImplFromRefSdkConfigForConfigBuilder(
    runtimeConfig: RuntimeConfig,
    rustCrate: RustCrate,
    customizations: List<AwsCustomization>,
) {
    val codegenContext = arrayOf("SdkConfig" to awsTypes(runtimeConfig).member("sdk_config::SdkConfig"))

    rustCrate.withModule(RustModule.Config) {
        rustBlockTemplate("impl From<&#{SdkConfig}> for Builder", *codegenContext) {
            rustBlockTemplate("fn from(input: &#{SdkConfig}) -> Self", *codegenContext) {
                rust("let mut builder = Builder::default();")
                writeCustomizations(customizations, AwsSection.FromSdkConfigForBuilder(customizations))
                rust("builder")
            }
        }
    }
}
