/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! HTTP server runtime and utilities, loosely based on [axum].
//!
//! [axum]: https://docs.rs/axum/latest/axum/

#[macro_use]
pub(crate) mod macros;

pub mod body;
pub(crate) mod error;
pub mod routing;

#[doc(hidden)]
pub mod extension;
#[doc(hidden)]
pub mod protocols;
#[doc(hidden)]
pub mod rejection;
#[doc(hidden)]
pub mod runtime_error;

#[doc(inline)]
pub(crate) use self::error::Error;
pub use self::extension::Extension;
#[doc(inline)]
pub use self::routing::Router;
#[doc(inline)]
pub use tower_http::add_extension::{AddExtension, AddExtensionLayer};

/// Alias for a type-erased error type.
pub(crate) use axum_core::BoxError;

#[cfg(test)]
mod test_helpers;
