//! Main CLI parser and top-level argument handling.
//!
//! This module defines the root CLI structure with global options.

use clap::Parser;

use crate::commands::Commands;

/// Command-line interface definition for the GGUF library management tool.
///
/// This is the top-level parser that handles global options and dispatches
/// to subcommands.
#[derive(Parser)]
#[command(name = "gglib")]
#[command(about = "Manage and run local GGUF models")]
#[command(version = gglib_build_info::LONG_VERSION)]
pub struct Cli {
    /// Override the models directory for this invocation
    #[arg(long = "models-dir", global = true)]
    pub models_dir: Option<String>,

    /// Enable verbose/debug output
    #[arg(short = 'v', long = "verbose", global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parser_builds() {
        // Verify the CLI parser can be constructed
        Cli::command().debug_assert();
    }

    #[test]
    fn test_global_args() {
        use clap::Parser;
        let cli = Cli::parse_from(["gglib", "--verbose", "--models-dir", "/tmp/models", "list"]);
        assert!(cli.verbose);
        assert_eq!(cli.models_dir, Some("/tmp/models".to_string()));
    }
}
