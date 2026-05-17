//! Command-line interface.
//!
//! `hoppr` (no subcommand) launches the TUI. Subcommands cover headless
//! workflows: scripted connects, listing, config management and sync.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "hoppr",
    version,
    about = "A fast, minimal TUI launcher for SSH and other remote shells.",
    long_about = None,
    arg_required_else_help = false,
)]
pub struct Cli {
    /// Path to an explicit config file (overrides the default location).
    #[arg(long, short = 'c', global = true, env = "HOPPR_CONFIG")]
    pub config: Option<PathBuf>,

    /// Skip the central git-repo pull on startup.
    #[arg(long, global = true)]
    pub no_sync: bool,

    /// Force a sync attempt even when disabled in the config.
    #[arg(long, global = true, conflicts_with = "no_sync")]
    pub sync: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Launch the interactive TUI (default).
    Tui,

    /// Connect to a host without opening the TUI.
    Connect(ConnectArgs),

    /// List configured categories and hosts.
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Inspect or edit the local configuration file.
    #[command(subcommand)]
    Config(ConfigCmd),

    /// Pull / push the centralized config repo.
    #[command(subcommand)]
    Sync(SyncCmd),

    /// Show recent connection history.
    History(HistoryArgs),

    /// Print shell completion script.
    Completions {
        /// Shell flavour.
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, clap::Args)]
pub struct ConnectArgs {
    /// Host name, IP or "category/host" — fuzzy-matched against the config.
    pub query: String,

    /// Override the user.
    #[arg(short = 'u', long)]
    pub user: Option<String>,

    /// Override the port.
    #[arg(short = 'p', long)]
    pub port: Option<u16>,

    /// Override the command (e.g. `mosh`, `telnet`).
    #[arg(long)]
    pub command: Option<String>,

    /// Print the resolved command instead of executing it.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Restrict to a single category (case-insensitive substring match).
    #[arg(short, long)]
    pub category: Option<String>,

    /// Output format.
    #[arg(short = 'o', long, value_enum, default_value_t = ListFormat::Table)]
    pub format: ListFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ListFormat {
    Table,
    Json,
    Yaml,
    Plain,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Print the active config file path.
    Path,
    /// Dump the resolved configuration to stdout.
    Show,
    /// Open the config file in `$EDITOR`.
    Edit,
    /// Write a starter config to the default location.
    Init {
        /// Overwrite an existing config file.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum SyncCmd {
    /// Pull the latest config from the central repo.
    Pull,
    /// Commit local edits and push them upstream.
    Push {
        /// Custom commit message.
        #[arg(short, long, default_value = "chore: update hoppr config")]
        message: String,
    },
    /// Show sync configuration and current state.
    Status,
}

#[derive(Debug, clap::Args)]
pub struct HistoryArgs {
    /// Maximum number of entries to show.
    #[arg(short = 'n', long, default_value_t = 20)]
    pub limit: usize,

    /// Output format.
    #[arg(short = 'o', long, value_enum, default_value_t = ListFormat::Table)]
    pub format: ListFormat,
}

impl Cli {
    pub fn parse_cli() -> Self {
        Self::parse()
    }
}
