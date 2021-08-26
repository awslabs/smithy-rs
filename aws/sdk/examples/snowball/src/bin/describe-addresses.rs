/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use aws_sdk_snowball::{Client, Config, Error, Region, PKG_VERSION};
use aws_types::region::{self};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    /// The AWS Region.
    #[structopt(short, long)]
    region: Option<String>,

    /// Whether to display additional information.
    #[structopt(short, long)]
    verbose: bool,
}

/// Lists your AWS Snowball addresses.
/// # Arguments
///
/// * `[-r REGION]` - The Region in which the client is created.
///    If not supplied, uses the value of the **AWS_REGION** environment variable.
///    If the environment variable is not set, defaults to **us-west-2**.
/// * `[-v]` - Whether to display additional information.
#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let Opt { region, verbose } = Opt::from_args();

    let region_provider = region::ChainProvider::first_try(region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));
    let region = region_provider.region().await;

    println!();

    if verbose {
        println!("Snowball version: {}", PKG_VERSION);
        println!("Region:           {}", region.as_ref().unwrap());
    }

    let conf = Config::builder().region(region).build();
    let client = Client::from_conf(conf);

    let addresses = client.describe_addresses().send().await?;
    for address in addresses.addresses.unwrap() {
        println!("Address: {:?}", address);
    }

    Ok(())
}
