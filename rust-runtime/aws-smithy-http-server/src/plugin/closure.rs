/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use tower::layer::util::Stack;

use crate::operation::{Operation, OperationShape};
use crate::shape_id::ShapeId;

use super::Plugin;

/// An adapter to convert a `Fn(ShapeId) -> Layer` closure into a [`Plugin`]. See [`plugin_from_operation_id_fn`] for more details.
pub struct OperationIdFn<F> {
    f: F,
}

impl<P, Op, S, ExistingLayer, NewLayer, F> Plugin<P, Op, S, ExistingLayer> for OperationIdFn<F>
where
    F: Fn(ShapeId) -> NewLayer,
    Op: OperationShape,
{
    type Service = S;
    type Layer = Stack<ExistingLayer, NewLayer>;

    fn map(&self, input: Operation<S, ExistingLayer>) -> Operation<Self::Service, Self::Layer> {
        let operation_id = Op::NAME;
        input.layer((self.f)(operation_id))
    }
}

/// Constructs a [`Plugin`] using a closure over the operation name `F: Fn(ShapeId) -> L` where `L` is a HTTP
/// [`Layer`](tower::Layer).
///
/// # Example
///
/// ```rust
/// use aws_smithy_http_server::plugin::plugin_from_operation_id_fn;
/// use aws_smithy_http_server::shape_id::ShapeId;
/// use tower::layer::layer_fn;
///
/// // A `Service` which prints the operation name before calling `S`.
/// struct PrintService<S> {
///     operation_name: ShapeId,
///     inner: S
/// }
///
/// // A `Layer` applying `PrintService`.
/// struct PrintLayer {
///     operation_name: ShapeId
/// }
///
/// // Defines a closure taking the operation name to `PrintLayer`.
/// let f = |operation_name| PrintLayer { operation_name };
///
/// // This plugin applies the `PrintService` middleware around every operation.
/// let plugin = plugin_from_operation_id_fn(f);
/// ```
pub fn plugin_from_operation_id_fn<L, F>(f: F) -> OperationIdFn<F>
where
    F: Fn(ShapeId) -> L,
{
    OperationIdFn { f }
}
