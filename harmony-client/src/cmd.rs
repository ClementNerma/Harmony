use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
pub struct Args {
    #[clap(help = "Directory to synchronize")]
    pub source_dir: PathBuf,

    #[clap(help = "Address of the server")]
    pub address: String,

    #[clap(help = "Slot name to use")]
    pub slot: String,

    #[clap(long, help = "Server's secret password")]
    pub secret: String,

    #[clap(long, help = "Device name")]
    pub device_name: Option<String>,

    #[clap(flatten)]
    pub sync_args: SyncArgs,

    #[clap(
        short,
        long,
        help = "Maximum number of parallel transfers (default: smaller between CPU cores and 8)"
    )]
    pub max_parallel_transfers: Option<usize>,

    #[clap(global = true, short, long, help = "Display debug messages")]
    pub verbose: bool,
}

#[derive(clap::Args)]
pub struct SyncArgs {
    #[clap(
        short,
        long,
        help = "Item names to ignore (start with a '/' for root-only)"
    )]
    pub ignore_items: Vec<String>,

    #[clap(long, help = "File extensions to ignore")]
    pub ignore_exts: Vec<String>,

    #[clap(long, help = "Perform a dry run")]
    pub dry_run: bool,
}
