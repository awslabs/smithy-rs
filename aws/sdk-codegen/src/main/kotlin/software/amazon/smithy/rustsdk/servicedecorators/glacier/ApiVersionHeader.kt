/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rustsdk.servicedecorators.glacier

import software.amazon.smithy.rust.codegen.core.rustlang.CargoDependency.Companion.Http
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.customize.OperationCustomization
import software.amazon.smithy.rust.codegen.core.smithy.customize.OperationSection
import software.amazon.smithy.rust.codegen.core.util.dq

class ApiVersionHeader(
    /**
     * ApiVersion
     * This usually comes from the `version` field of the service shape and is usually a date like "2012-06-01"
     * */
    private val apiVersion: String,
) : OperationCustomization() {
    override fun section(section: OperationSection): Writable = when (section) {
        is OperationSection.MutateRequest -> writable {
            rustTemplate(
                """
                ${section.request}
                    .http_mut()
                    .headers_mut()
                    .insert("x-amz-glacier-version", #{HeaderValue}::from_static(${apiVersion.dq()}));
                """,
                "HeaderValue" to Http.asType().resolve("HeaderValue"),
            )
        }
        else -> emptySection
    }
}
