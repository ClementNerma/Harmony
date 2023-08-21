use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use log::LevelFilter;

use crate::paths::SlotInfos;

#[derive(Parser)]
pub struct Args {
    #[clap(short, long, help = "Synchronization directory")]
    pub data_dir: PathBuf,

    #[clap(flatten)]
    pub backup_args: BackupArgs,

    #[clap(flatten)]
    pub http_args: HttpArgs,

    #[clap(short, long, help = "Logging level", default_value = "info")]
    pub logging_level: LevelFilter,
}

#[derive(clap::Args)]
pub struct HttpArgs {
    #[clap(short, long, help = "Address to listen on", default_value = "0.0.0.0")]
    pub addr: IpAddr,

    #[clap(short, long, help = "Port to listen on", default_value = "9423")]
    pub port: u16,
}

#[derive(clap::Args)]
pub struct BackupArgs {
    #[clap(short, long, help = "List of available slots")]
    pub slots: Vec<SlotInfos>,

    #[clap(long, help = "The secret password")]
    pub secret: String,

    #[clap(short, long, help = "Number of backups to keep", default_value = "5")]
    pub backups_limit: usize,
}
