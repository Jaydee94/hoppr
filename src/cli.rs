//! Command-line interface.
//!
//! `hoppr` (no subcommand) launches the TUI. Subcommands cover headless
//! workflows: scripted connects, listing, config management and sync.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use clap::{
    CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum, error::ErrorKind,
    parser::ValueSource,
};
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
    /// Before the subcommand it can be shortened to `-c <PATH>`.
    #[arg(long, global = true, env = "HOPPR_CONFIG", value_name = "PATH")]
    config: Option<PathBuf>,

    // Not global: subcommands are free to give `-c` their own meaning
    // (`list -c <CATEGORY>`), and clap rejects a short shared between a
    // global arg and a subcommand arg. Hidden so `--help` shows a single
    // config entry; the `--config` help text mentions this form.
    #[arg(short = 'c', value_name = "PATH", hide = true)]
    config_short: Option<PathBuf>,

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
        Self::try_parse_cli_from(std::env::args_os()).unwrap_or_else(|err| err.exit())
    }

    /// Parses `args` and rejects `-c` combined with `--config`.
    ///
    /// The check is done here rather than with clap's `conflicts_with`
    /// because clap counts a value taken from `$HOPPR_CONFIG` as present,
    /// which would turn `-c <PATH>` into an error whenever the variable is
    /// exported instead of letting the flag override it.
    pub fn try_parse_cli_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let mut command = Self::command();
        let matches = command.try_get_matches_from_mut(args)?;
        let cli = Self::from_arg_matches(&matches).map_err(|err| err.format(&mut command))?;
        let long_on_command_line = matches.value_source("config") == Some(ValueSource::CommandLine);
        if cli.config_short.is_some() && long_on_command_line {
            return Err(command.error(
                ErrorKind::ArgumentConflict,
                "the argument '-c <PATH>' cannot be used with '--config <PATH>'",
            ));
        }
        Ok(cli)
    }

    /// The explicit config path, whichever spelling supplied it: `-c` before
    /// the subcommand, `--config` anywhere, or `$HOPPR_CONFIG`.
    pub fn config_path(&self) -> Option<&Path> {
        self.config_short.as_deref().or(self.config.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_cli_from(args).expect("arguments parse")
    }

    fn list_category(cli: &Cli) -> Option<&str> {
        match cli.command.as_ref() {
            Some(Command::List(args)) => args.category.as_deref(),
            _ => None,
        }
    }

    #[test]
    fn command_definition_passes_clap_debug_asserts() {
        Cli::command().debug_assert();
    }

    #[test]
    fn short_config_flag_is_accepted_before_the_subcommand() {
        let cli = parse(&["hoppr", "-c", "p"]);
        assert_eq!(cli.config_path(), Some(Path::new("p")));
    }

    #[test]
    fn long_config_flag_is_accepted() {
        let cli = parse(&["hoppr", "--config", "p"]);
        assert_eq!(cli.config_path(), Some(Path::new("p")));
    }

    #[test]
    fn short_c_means_config_before_list_and_category_after() {
        let cli = parse(&["hoppr", "-c", "p", "list", "-c", "prod"]);
        assert_eq!(cli.config_path(), Some(Path::new("p")));
        assert_eq!(list_category(&cli), Some("prod"));
    }

    #[test]
    fn long_config_flag_is_global_after_the_subcommand() {
        let cli = parse(&["hoppr", "list", "--config", "p", "-c", "prod"]);
        assert_eq!(cli.config_path(), Some(Path::new("p")));
        assert_eq!(list_category(&cli), Some("prod"));
    }

    #[test]
    fn short_and_long_config_flags_conflict() {
        for args in [
            ["hoppr", "-c", "a", "--config", "b", "list"],
            ["hoppr", "-c", "a", "list", "--config", "b"],
        ] {
            let err = Cli::try_parse_cli_from(args).expect_err("both spellings are rejected");
            assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
        }
    }
}
