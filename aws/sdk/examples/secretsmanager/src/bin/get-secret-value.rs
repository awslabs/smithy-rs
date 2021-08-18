/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */
use std::process;

use secretsmanager::{Client, Config, Region};

use aws_types::region::{EnvironmentProvider, ProvideRegion};

use structopt::StructOpt;

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::SubscriberBuilder;

#[derive(Debug, StructOpt)]
struct Opt {
    /// The region
    #[structopt(short, long)]
    region: Option<String>,

    /// The name of the secret
    #[structopt(short, long)]
    name: String,

    /// Whether to display additonal runtime information
    #[structopt(short, long)]
    verbose: bool,
}

/// Retrieves the value of a secret.
/// # Arguments
///
/// * `-n NAME` - The name of the secret.
/// * `-s SECRET_VALUE` - The secret value.
/// * `[-d DEFAULT-REGION]` - The region in which the client is created.
///    If not supplied, uses the value of the **AWS_DEFAULT_REGION** environment variable.
///    If the environment variable is not set, defaults to **us-west-2**.
/// * `[-v]` - Whether to display additional information.
#[tokio::main]
async fn main() {
    let Opt {
        name,
        region,
        verbose,
    } = Opt::from_args();

    let region = EnvironmentProvider::new()
        .region()
        .await
        .or_else(|| region.as_ref().map(|region| Region::new(region.clone())))
        .unwrap_or_else(|| Region::new("us-west-2"));

    if verbose {
        println!(
            "SecretsManager client version: {}\n",
            secretsmanager::PKG_VERSION
        );
        println!("Region:      {:?}", region.region().await);
        println!("Secret name: {}", name);

        SubscriberBuilder::default()
            .with_env_filter("info")
            .with_span_events(FmtSpan::CLOSE)
            .init();
    }

    let config = Config::builder().region(region).build().await;
    let client = Client::from_conf(config);

    match client.get_secret_value().secret_id(name).send().await {
        Ok(resp) => {
            println!(
                "Value: {}",
                resp.secret_string.as_deref().unwrap_or("No value!")
            );
        }
        Err(e) => {
            println!("Got an error listing secrets:");
            println!("{}", e);
            process::exit(1);
        }
    };
}
