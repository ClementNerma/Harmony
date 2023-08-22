#![forbid(unsafe_code)]
#![forbid(unused_must_use)]
#![warn(unused_crate_dependencies)]

use self::cmd::Args;
use anyhow::{bail, Context, Result};
use clap::Parser;
use colored::Colorize;
use data::AppData;
use log::{debug, error, info};
use paths::Paths;
use tokio::fs;

// Vendor OpenSSL inside the binary to avoid dependencies problem
use openssl as _;

mod cmd;
mod data;
mod http;
mod paths;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    env_logger::builder()
        .filter_level(args.logging_level)
        .init();

    debug!("Application is starting...");

    if let Err(err) = inner_main(args).await {
        error!("{err:?}");
        std::process::exit(1);
    }
}

async fn inner_main(args: Args) -> Result<()> {
    let Args {
        data_dir,
        backup_args,
        http_args,
        logging_level: _,
    } = args;

    if !data_dir.is_dir() {
        bail!("Provided data directory does not exist");
    }

    let paths = Paths::new(data_dir.clone());

    let app_data_file = paths.app_data_file();

    let app_data = if app_data_file.exists() {
        AppData::load(&app_data_file).await?
    } else {
        AppData::empty()
    };

    if backup_args.slots.is_empty() {
        bail!("Please provide at least one backup slot");
    }

    for slot in &backup_args.slots {
        let slot_dir = paths.slot_root_dir(slot);

        if !slot_dir.is_dir() {
            fs::create_dir_all(&slot_dir).await.with_context(|| {
                format!(
                    "Failed to create slot data directory at: {}",
                    slot_dir.display()
                )
            })?;
        }

        let slot_files_dir = paths.slot_content_dir(slot);

        if !slot_files_dir.is_dir() {
            fs::create_dir_all(&slot_files_dir).await.with_context(|| {
                format!(
                    "Failed to create slot content directory at: {}",
                    slot_files_dir.display()
                )
            })?;
        }

        info!("Slot {} is ready", slot.name().bright_blue());
    }

    http::launch(http_args, backup_args, app_data, paths).await
}
