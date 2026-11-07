use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "anvil",
    author = "Anvil Contributors",
    version,
    about = "Zero-overhead homelab provisioning & telemetry engine",
    long_about = "Anvil is a single-binary post-install bootstrap daemon and terminal dashboard that auto-integrates with Linux system primitives, storage pools, and container sockets."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub read_only: bool,
    #[arg(long, global = true, value_name = "FILE")]
    pub log_file: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Tui {
        #[arg(short, long, default_value_t = 1000)]
        refresh_rate: u64,
    },
    Discover {
        #[arg(long)]
        json: bool,
    },
    Storage {
        #[arg(long)]
        json: bool,
    },
    Runtime {
        #[arg(long)]
        json: bool,
    },
    Plugins {
        #[arg(long)]
        json: bool,
    },
    Apply {
        #[arg(
            short,
            long,
            value_name = "FILE",
            default_value = "/etc/anvil/manifest.toml"
        )]
        manifest: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(short, long, action = clap::ArgAction::Count)]
        verbose: u8,
    },
}
