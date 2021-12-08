/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Core HTTP primitives for service clients generated by [smithy-rs](https://github.com/awslabs/smithy-rs) including:
//! - HTTP Body implementation
//! - Endpoint support
//! - HTTP header deserialization
//! - Event streams
//! - `ByteStream`: _(supported on crate feature `rt-tokio` only)_ a misuse-resistant abstraction for streaming binary data
//!
//! | Feature        | Description |
//! |----------------|-------------|
//! | `rt-tokio`     | Provides features that are dependent on `tokio` including the `ByteStream` utils |
//! | `event-stream` | Provides Sender/Receiver implementations for Event Stream codegen. |

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod body;
pub mod endpoint;
pub mod header;
pub mod label;
pub mod middleware;
pub mod operation;
pub mod property_bag;
pub mod query;
pub mod response;
pub mod result;
pub mod retry;

#[cfg(feature = "event-stream")]
pub mod event_stream;

pub mod byte_stream;

mod pin_util;
mod urlencode;
