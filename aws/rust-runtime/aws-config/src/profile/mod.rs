/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

//! Load configuration from AWS Profiles
//!
//! AWS profiles are typically stored in `~/.aws/config` and `~/.aws/credentials`. For more details
//! see <todo>

mod parser;
pub use parser::{load, Profile, ProfileSet, Property};

mod credentials;
pub use credentials::ProfileFileCredentialsProvider;
/*
pub mod credential;

pub mod region;

pub use parser::{Profile, ProfileSet, Property};*/
