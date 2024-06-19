/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.traits

import software.amazon.smithy.model.node.Node
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.traits.AnnotationTrait

/**
 * Indicates that an operation should use the IsTruncated field for detecting the end of pagination.
 */
class IsTruncatedPaginatorTrait : AnnotationTrait(ID, Node.objectNode()) {
    companion object {
        val ID: ShapeId =
            ShapeId.from("software.amazon.smithy.rust.codegen.client.smithy.traits#isTruncatedPaginatorTrait")
    }
}
