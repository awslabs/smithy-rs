/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Conversions between `aws-smithy-types` and the types of frequently used Rust libraries.

#![warn(
    missing_docs,
    missing_crate_level_docs,
    missing_debug_implementations,
    rust_2018_idioms,
    unreachable_pub
)]

#[cfg(any(feature = "convert-time", feature = "convert-chrono"))]
pub mod instant;
