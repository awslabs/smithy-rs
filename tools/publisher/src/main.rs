/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use crate::subcommand::fix_manifests::{subcommand_fix_manifests, Mode};
use crate::subcommand::publish::subcommand_publish;
use crate::subcommand::yank_category::subcommand_yank_category;
use anyhow::Result;
use clap::{crate_authors, crate_description, crate_name, crate_version};

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "error,publisher=info".to_owned()),
        )
        .init();

    let matches = clap_app().get_matches();
    if let Some(matches) = matches.subcommand_matches("publish") {
        subcommand_publish(matches.value_of("location").unwrap()).await?;
    } else if let Some(fix_manifests) = matches.subcommand_matches("fix-manifests") {
        let mode = match fix_manifests.is_present("check") {
            true => Mode::Check,
            false => Mode::Execute,
        };
        subcommand_fix_manifests(mode, fix_manifests.value_of("location").unwrap()).await?;
    } else if let Some(matches) = matches.subcommand_matches("yank-category") {
        let category = matches.value_of("category").unwrap();
        let version = matches.value_of("version").unwrap();
        let location = matches.value_of("location").unwrap();
        subcommand_yank_category(category, version, location).await?;
    } else {
        clap_app().print_long_help().unwrap();
    }
    Ok(())
}

fn clap_app() -> clap::App<'static, 'static> {
    clap::App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        // In the future, there may be another subcommand for yanking
        .subcommand(
            clap::SubCommand::with_name("fix-manifests")
                .about("fixes path dependencies in manifests to also have version numbers")
                .arg(
                    clap::Arg::with_name("location")
                        .required(true)
                        .takes_value(true)
                        .long("location")
                        .help("Path containing the manifests to fix. Manifests will be discovered recursively"),
                )
                .arg(
                    clap::Arg::with_name("check")
                        .required(false)
                        .takes_value(false)
                        .long("check"),
                ),
        )
        .subcommand(
            clap::SubCommand::with_name("publish")
                .about("publishes crates to crates.io")
                .arg(
                    clap::Arg::with_name("location")
                        .required(true)
                        .takes_value(true)
                        .long("location")
                        .help("Path containing the crates to publish. Crates will be discovered recursively"),
                )
        )
        .subcommand(
            clap::SubCommand::with_name("yank-category")
                .about("yanks a category of packages with the given version number")
                .arg(
                    clap::Arg::with_name("category")
                        .long("category")
                        .required(true)
                        .takes_value(true)
                        .help("package category to yank (smithy-runtime, aws-runtime, or aws-sdk)"),
                )
                .arg(
                    clap::Arg::with_name("version")
                        .long("version")
                        .required(true)
                        .takes_value(true)
                        .help("version number to yank"),
                )
                .arg(
                    clap::Arg::with_name("location")
                        .required(true)
                        .takes_value(true)
                        .long("location")
                        .help("Path to `aws-sdk-rust` repo. The repo should be checked out at the \
                               version that is being yanked so that the correct list of crate names \
                               is used. This will be validated.")
                )
        )
}
