/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.transformers

import software.amazon.smithy.model.Model
import software.amazon.smithy.model.neighbor.Walker
import software.amazon.smithy.model.shapes.EnumShape
import software.amazon.smithy.model.shapes.ListShape
import software.amazon.smithy.model.shapes.MapShape
import software.amazon.smithy.model.shapes.SetShape
import software.amazon.smithy.model.shapes.StringShape
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.shapes.UnionShape
import software.amazon.smithy.model.transform.ModelTransformer
import software.amazon.smithy.rust.codegen.core.smithy.hasConstraintTrait
import software.amazon.smithy.rust.codegen.core.smithy.traits.AggregateShapeReachableFromOperationInputTagTrait
import software.amazon.smithy.rust.codegen.core.util.UNREACHABLE

/**
 * Tag all [aggregate shapes] reachable from operation input with the
 * [AggregateShapeReachableFromOperationInputTagTrait] tag.
 *
 * This is useful to determine whether we need to generate code to
 * enforce constraints upon request deserialization in the server.
 *
 * This needs to be a model transformer; it cannot be lazily calculated
 * when needed. This is because other model transformers may transform
 * the model such that aggregate shapes that were reachable from operation
 * input are no longer so. For example, [EventStreamNormalizer] pulls
 * event stream error variants out of the union shape where they are defined.
 * As such, [AggregateShapesReachableFromOperationInputTagger] needs to run
 * before these model transformers.
 *
 * [aggregate shapes]: https://awslabs.github.io/smithy/2.0/spec/aggregate-types.html#aggregate-types
 *
 * TODO Move this to `server`.
 */
object AggregateShapesReachableFromOperationInputTagger {
    fun transform(model: Model): Model {
        val inputShapes = model.operationShapes.map { model.expectShape(it.inputShape, StructureShape::class.java) }
        val walker = Walker(model)
        val shapesReachableFromOperationInputs = inputShapes
            .flatMap { walker.walkShapes(it) }
            .toSet()
        val shapesReachableFromConstrainedOperationInputs = shapesReachableFromOperationInputs
            .filter { it is SetShape || it is EnumShape || it.hasConstraintTrait() }

        return ModelTransformer.create().mapShapes(model) { shape ->
            when (shape) {
                is StructureShape, is UnionShape, is ListShape, is MapShape, is StringShape -> {
                    val builder = when (shape) {
                        is StructureShape -> shape.toBuilder()
                        is UnionShape -> shape.toBuilder()
                        is ListShape -> shape.toBuilder()
                        is MapShape -> shape.toBuilder()
                        is StringShape -> shape.toBuilder()
                        else -> UNREACHABLE("the `when` is exhaustive")
                    }

                    if (shapesReachableFromOperationInputs.contains(shape)) {
                        builder.addTrait(AggregateShapeReachableFromOperationInputTagTrait()).build()
                    } else if (shapesReachableFromConstrainedOperationInputs.contains(shape)) {
                        builder.addTrait(AggregateShapeReachableFromOperationInputTagTrait()).build()
                    } else {
                        shape
                    }
                }
                else -> shape
            }
        }
    }
}
