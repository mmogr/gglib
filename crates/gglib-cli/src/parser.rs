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
// `{about-with-newline}` resolves to the *long* about under `--help`, and clap
// derives that from this struct's doc comment when none is set. So `gglib
// --help` opened with "Command-line interface definition ... top-level parser
// that handles global options and dispatches to subcommands" — accurate about
// the type, addressed to nobody who runs the binary.
#[command(long_about = "Manage and run local GGUF models")]
#[command(version = gglib_build_info::LONG_VERSION)]
#[command(subcommand_help_heading = "Commands")]
#[command(disable_help_subcommand = true)]
#[command(
    help_template = "{before-help}{name} {version}\n{about-with-newline}\n\
{usage-heading} {usage}\n\n\
Getting started:\n  \
up              Set up everything and start a working endpoint (start here)\n\n\
Management (use <command> --help to see subcommands):\n  \
model           Manage GGUF models (add, list, remove, download, verify, \u{2026})\n  \
config          Manage configuration, tooling, and system settings\n  \
mcp             Manage MCP (Model Context Protocol) tool servers\n\n\
Inference:\n  \
serve           Serve one pinned model behind the OpenAI-compatible proxy\n  \
chat            Chat with a model interactively\n  \
question        Ask a question with optional context from stdin or file\n\n\
Interfaces:\n  \
gui             Launch the Tauri desktop GUI\n  \
web             Ensure the daemon is up and print its URL\n  \
proxy           Start OpenAI-compatible proxy with MCP tool gateway\n  \
daemon          Run, inspect and stop the background daemon\n\n\
Measurement:\n  \
benchmark       Measure a model, and tune its sampling defaults\n\n\
Shell:\n  \
completions     Generate a shell completion script\n\n\
Options:\n{options}{after-help}"
)]
pub struct Cli {
    /// Override the models directory for this invocation
    #[arg(long = "models-dir", global = true)]
    pub models_dir: Option<String>,

    /// Enable verbose logging (debug level + file output to logs/)
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
        let cli = Cli::parse_from([
            "gglib",
            "--verbose",
            "--models-dir",
            "/tmp/models",
            "model",
            "list",
        ]);
        assert!(cli.verbose);
        assert_eq!(cli.models_dir, Some("/tmp/models".to_string()));
    }

    #[test]
    fn test_up_parses_its_flags() {
        use clap::Parser;
        let cli = Cli::parse_from(["gglib", "up", "--yes", "--model", "qwen", "--port", "9999"]);
        match cli.command {
            Some(crate::commands::Commands::Up { yes, model, port }) => {
                assert!(yes);
                assert_eq!(model.as_deref(), Some("qwen"));
                assert_eq!(port, 9999);
            }
            _ => panic!("expected the Up variant"),
        }
    }

    /// The command list in `help_template` is a hand-written string, so adding
    /// a variant to `Commands` does not add it to `--help`.
    ///
    /// This checked only `up`, and three commands shipped invisible behind it:
    /// `benchmark`, `completions`, and `daemon` — the last being the headline
    /// concept of the whole daemon consolidation. Asking clap for the variant
    /// list instead means a new command cannot be forgotten, only deliberately
    /// exempted, and there is no exemption list.
    #[test]
    fn every_command_appears_in_the_top_level_help() {
        let mut command = Cli::command();
        let help = command.render_help().to_string();

        // Match the *listing* — an indented line whose first word is the
        // command — not the name anywhere in the text. A plain `contains`
        // passes on prose: "web  Ensure the daemon is up" contains "daemon ",
        // so dropping the `daemon` row would still have looked fine.
        let listed: Vec<&str> = help
            .lines()
            .filter_map(|line| line.strip_prefix("  "))
            .filter_map(|line| line.split_whitespace().next())
            .collect();

        let missing: Vec<_> = command
            .get_subcommands()
            .map(clap::Command::get_name)
            .filter(|name| !listed.contains(name))
            .collect();

        assert!(
            missing.is_empty(),
            "missing from the hand-written help template: {missing:?}\n{help}"
        );
    }

    /// A template that named nothing would pass the check above by matching
    /// zero subcommands against an empty list, which is how a guard reports
    /// success for having stopped working.
    #[test]
    fn the_help_template_names_something() {
        let command = Cli::command();
        assert!(
            command.get_subcommands().count() >= 10,
            "the subcommand list collapsed; the help guard is checking nothing"
        );
    }
}
