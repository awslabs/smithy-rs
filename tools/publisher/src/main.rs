/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use crate::subcommand::fix_manifests::{subcommand_fix_manifests, Mode};
use crate::subcommand::publish::subcommand_publish;
use crate::subcommand::yank_category::subcommand_yank_category;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod cargo;
mod fs;
mod git;
mod package;
mod repo;
mod shell;
mod sort;
mod subcommand;

pub const REPO_NAME: &str = "aws-sdk-rust";
pub const REPO_CRATE_PATH: &str = "sdk";
pub const CRATE_OWNER: &str = "github:awslabs:rust-sdk-owners";

#[derive(Parser, Debug)]
#[clap(author, version, about)]
enum Args {
    /// Fixes path dependencies in manifests to also have version numbers
    FixManifests {
        /// Path containing the manifests to fix. Manifests will be discovered recursively
        #[clap(long)]
        location: PathBuf,
        /// Checks manifests rather than fixing them
        #[clap(long)]
        check: bool,
    },
    /// Publishes crates to crates.io
    Publish {
        /// Path containing the crates to publish. Crates will be discovered recursively
        #[clap(long)]
        location: PathBuf,
    },
    /// Yanks a category of packages with the given version number
    YankCategory {
        /// Package category to yank (smithy-runtime, aws-runtime, or aws-sdk)
        #[clap(long)]
        category: String,
        /// Version number to yank
        #[clap(long)]
        version: String,
        /// Path to `aws-sdk-rust` repo. The repo should be checked out at the
        /// version that is being yanked so that the correct list of crate names
        /// is used. This will be validated.
        #[clap(long)]
        location: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "error,publisher=info".to_owned()),
        )
        .init();

    match Args::parse() {
        Args::Publish { location } => {
            subcommand_publish(&location).await?;
        }
        Args::FixManifests { location, check } => {
            let mode = match check {
                true => Mode::Check,
                false => Mode::Execute,
            };
            subcommand_fix_manifests(mode, &location).await?;
        }
        Args::YankCategory {
            category,
            version,
            location,
        } => {
            subcommand_yank_category(&category, &version, &location).await?;
        }
    }
    Ok(())
}
