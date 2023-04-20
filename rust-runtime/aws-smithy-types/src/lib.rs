/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Protocol-agnostic types for smithy-rs.

#![allow(clippy::derive_partial_eq_without_eq)]
#![warn(
    missing_docs,
    rustdoc::missing_crate_level_docs,
    missing_debug_implementations,
    rust_2018_idioms,
    unreachable_pub
)]

use crate::error::{TryFromNumberError, TryFromNumberErrorKind};
use std::collections::HashMap;

pub mod base64;
pub mod date_time;
pub mod endpoint;
pub mod error;
pub mod primitive;
pub mod retry;
pub mod timeout;

pub use blob::Blob;
pub use date_time::DateTime;
pub use document::Document;
pub use error::ErrorMetadata as Error;
pub use number::Number;
