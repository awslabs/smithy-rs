/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Provides an example [`Plugin`] implementation - [`PrintPlugin`].

use aws_smithy_http_server::{
    operation::OperationShape,
    plugin::{Plugin, PluginPipeline, PluginStack},
    service::ServiceShape,
    shape_id::ShapeId,
};
use tower::Service;

use std::task::{Context, Poll};

/// A [`Service`] that prints a given string.
#[derive(Clone, Debug)]
pub struct PrintService<S> {
    inner: S,
    operation_id: ShapeId,
    service_id: ShapeId,
}

impl<R, S> Service<R> for PrintService<S>
where
    S: Service<R>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R) -> Self::Future {
        println!(
            "Hi {} in {}",
            self.operation_id.absolute(),
            self.service_id.absolute()
        );
        self.inner.call(req)
    }
}
/// A [`Plugin`] for a service builder to add a [`PrintLayer`] over operations.
#[derive(Debug)]
pub struct PrintPlugin;

impl<Ser, Op, T> Plugin<Ser, Op, T> for PrintPlugin
where
    Ser: ServiceShape,
    Op: OperationShape,
{
    type Output = PrintService<T>;

    fn apply(&self, inner: T) -> Self::Output {
        PrintService {
            inner,
            operation_id: Op::ID,
            service_id: Ser::ID,
        }
    }
}
/// This provides a [`print`](PrintExt::print) method on [`PluginPipeline`].
pub trait PrintExt<CurrentPlugin> {
    /// Causes all operations to print the operation name when called.
    ///
    /// This works by applying the [`PrintPlugin`].
    fn print(self) -> PluginPipeline<PluginStack<PrintPlugin, CurrentPlugin>>;
}

impl<CurrentPlugin> PrintExt<CurrentPlugin> for PluginPipeline<CurrentPlugin> {
    fn print(self) -> PluginPipeline<PluginStack<PrintPlugin, CurrentPlugin>> {
        self.push(PrintPlugin)
    }
}
