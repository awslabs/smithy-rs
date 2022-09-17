/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

mod handler;
mod layer;
mod request;
mod response;

use aws_smithy_http_server::body::{Body, BoxBody};
use futures::future::BoxFuture;
use http::{Request, Response};

pub use self::handler::{PyMiddlewareHandler, PyMiddlewares};
pub use self::layer::PyMiddlewareLayer;
pub use self::request::{PyHttpVersion, PyRequest};
pub use self::response::PyResponse;

pub type PyFuture = BoxFuture<'static, Result<Request<Body>, Response<BoxBody>>>;
