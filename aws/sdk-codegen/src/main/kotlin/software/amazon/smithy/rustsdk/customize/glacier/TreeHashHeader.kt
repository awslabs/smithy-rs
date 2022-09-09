/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rustsdk.customize.glacier

import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.rust.codegen.client.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.client.rustlang.Writable
import software.amazon.smithy.rust.codegen.client.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.client.rustlang.writable
import software.amazon.smithy.rust.codegen.client.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.client.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.client.smithy.customize.OperationCustomization
import software.amazon.smithy.rust.codegen.client.smithy.customize.OperationSection
import software.amazon.smithy.rust.codegen.client.smithy.generators.operationBuildError
import software.amazon.smithy.rust.codegen.client.testutil.TokioWithTestMacros
import software.amazon.smithy.rustsdk.InlineAwsDependency

val TreeHashDependencies = listOf(
    CargoDependency.Ring,
    CargoDependency.TokioStream,
    CargoDependency.BytesUtils,
    CargoDependency.Bytes,
    TokioWithTestMacros,
    CargoDependency.Hex,
    CargoDependency.TempFile,
)

private val UploadArchive: ShapeId = ShapeId.from("com.amazonaws.glacier#UploadArchive")
private val UploadMultipartPart: ShapeId = ShapeId.from("com.amazonaws.glacier#UploadMultipartPart")
private val Applies = setOf(UploadArchive, UploadMultipartPart)

class TreeHashHeader(private val runtimeConfig: RuntimeConfig) : OperationCustomization() {
    private val glacierChecksums = RuntimeType.forInlineDependency(InlineAwsDependency.forRustFile("glacier_checksums"))
    override fun section(section: OperationSection): Writable {
        return when (section) {
            is OperationSection.MutateRequest -> writable {
                TreeHashDependencies.forEach { dep ->
                    addDependency(dep)
                }
                rustTemplate(
                    """
                    #{glacier_checksums}::add_checksum_treehash(
                        &mut ${section.request}
                    ).await.map_err(|e|#{BuildError}::Other(e.into()))?;
                    """,
                    "glacier_checksums" to glacierChecksums, "BuildError" to runtimeConfig.operationBuildError(),
                )
            }
            else -> emptySection
        }
    }

    companion object {
        fun forOperation(operation: OperationShape, runtimeConfig: RuntimeConfig): TreeHashHeader? {
            return if (Applies.contains(operation.id)) {
                TreeHashHeader(runtimeConfig)
            } else {
                null
            }
        }
    }
}
