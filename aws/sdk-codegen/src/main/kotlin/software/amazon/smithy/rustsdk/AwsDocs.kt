/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rustsdk

import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.docsTemplate
import software.amazon.smithy.rust.codegen.core.util.toSnakeCase

object AwsDocs {
    /**
     * If no `aws-config` version is provided, assume that docs referencing `aws-config` cannot be given.
     * Also, STS and SSO must NOT reference `aws-config` since that would create a circular dependency.
     */
    fun canRelyOnAwsConfig(codegenContext: ClientCodegenContext): Boolean =
        SdkSettings.from(codegenContext.settings).awsConfigVersion != null &&
            !setOf(
                ShapeId.from("com.amazonaws.sts#AWSSecurityTokenServiceV20110615"),
                ShapeId.from("com.amazonaws.sso#SWBPortalService"),
            ).contains(codegenContext.serviceShape.id)

    fun clientConstructionDocs(codegenContext: ClientCodegenContext): Writable = {
        if (canRelyOnAwsConfig(codegenContext)) {
            val crateName = codegenContext.moduleName.toSnakeCase()
            docsTemplate(
                """
                #### Constructing a `Client`

                A [`Config`] is required to construct a client. For most use cases, the [`aws-config`]
                crate should be used to automatically resolve this config using
                [`aws_config::load_from_env()`], since this will resolve an [`SdkConfig`] which can be shared
                across multiple different AWS SDK clients. This config resolution process can be customized
                by calling [`aws_config::from_env()`] instead, which returns a [`ConfigLoader`] that uses
                the [builder pattern] to customize the default config.

                In the simplest case, creating a client looks as follows:
                ```rust,no_run
                ## async fn wrapper() {
                let config = #{aws_config}::load_from_env().await;
                let client = $crateName::Client::new(&config);
                ## }
                ```

                Occasionally, SDKs may have additional service-specific that can be set on the [`Config`] that
                is absent from [`SdkConfig`], or slightly different settings for a specific client may be desired.
                The [`Config`] struct implements `From<&SdkConfig>`, so setting these specific settings can be
                done as follows:

                ```rust,no_run
                let sdk_config = #{aws_config}::load_from_env().await;
                let config = $crateName::Config::from(&sdk_config).to_builder()
                ## /*
                    .some_service_specific_setting("value")
                ## */
                    .build();
                ```

                See the [`aws-config` docs] and [`Config`] for more information on customizing configuration.

                _Note:_ Client construction is expensive due to connection thread pool initialization, and should
                be done once at application start-up.

                [`Config`]: crate::Config
                [`ConfigLoader`]: https://docs.rs/aws-config/*/aws_config/struct.ConfigLoader.html
                [`SdkConfig`]: https://docs.rs/aws-config/*/aws_config/struct.SdkConfig.html
                [`aws-config` docs]: https://docs.rs/aws-config/*
                [`aws-config`]: https://crates.io/crates/aws-config
                [`aws_config::from_env()`]: https://docs.rs/aws-config/*/aws_config/fn.from_env.html
                [`aws_config::load_from_env()`]: https://docs.rs/aws-config/*/aws_config/fn.load_from_env.html
                [builder pattern]: https://rust-lang.github.io/api-guidelines/type-safety.html##builders-enable-construction-of-complex-values-c-builder
                """.trimIndent(),
                "aws_config" to AwsCargoDependency.awsConfig(codegenContext.runtimeConfig).toDevDependency().toType(),
            )
        }
    }
}
