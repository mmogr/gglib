//! Main CLI parser and top-level argument handling.
//!
//! This module defines the root CLI structure with global options.

use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory as _, Parser};

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
q               Ask a question with optional context from stdin or file\n  \
run             Start a reply the daemon finishes, and read it later\n  \
image           Draw an image with this machine's image model, or the paired one's\n  \
attachment      Save an image a chat stored, such as one a tool made\n\n\
Interfaces:\n  \
gui             Launch the Tauri desktop GUI\n  \
web             Ensure the daemon is up and print its URL\n  \
proxy           Start OpenAI-compatible proxy with MCP tool gateway\n  \
daemon          Run, inspect and stop the background daemon\n  \
remote          Put this machine's proxy on another machine, securely\n\n\
Measurement:\n  \
benchmark       Measure a model, and tune its sampling defaults\n\n\
Shell:\n  \
completions     Generate a shell completion script\n\n\
Options:\n{options}{after-help}"
)]
pub struct Cli {
    /// Enable verbose logging (debug level + file output to logs/)
    #[arg(short = 'v', long = "verbose", global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// The sentence for a parent's own flags typed before its subcommand
/// (`gglib proxy --port 8081 dashboard`), which the subcommand never
/// receives; `None` when there are none. A global flag belongs to every
/// level, and a value from a default or the environment was not typed.
pub(crate) fn before_subcommand(matches: &ArgMatches) -> Option<String> {
    let root = Cli::command();
    let (mut command, mut here, mut path) = (&root, matches, vec!["gglib"]);
    while let Some((name, sub)) = here.subcommand() {
        let typed: Vec<String> = command
            .get_arguments()
            .filter(|arg| !arg.is_global_set())
            .filter(|arg| {
                here.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine)
            })
            .map(|arg| {
                // A positional is named by what was typed for it.
                let typed_value = || {
                    here.get_raw(arg.get_id().as_str())
                        .into_iter()
                        .flatten()
                        .map(|value| format!("`{}`", value.to_string_lossy()))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                arg.get_long()
                    .map_or_else(typed_value, |long| format!("--{long}"))
            })
            .collect();
        if let Some((named, them)) = named(&typed) {
            let parent = path.join(" ");
            return Some(format!(
                "`{parent} {name}` takes none of `{parent}`'s own arguments, so {named} before \
                 `{name}` would be dropped; drop {them}, or type {them} after `{name}` if it \
                 takes {them}"
            ));
        }
        command = command.find_subcommand(name)?;
        here = sub;
        path.push(name);
    }
    None
}

/// `names` as a sentence lists them (`a`, `a and b`, `a, b and c`), and the
/// pronoun for them; `None` when there are none.
pub(crate) fn named(names: &[String]) -> Option<(String, &'static str)> {
    let (last, rest) = names.split_last()?;
    Some(if rest.is_empty() {
        (last.clone(), "it")
    } else {
        (format!("{} and {last}", rest.join(", ")), "them")
    })
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
        let cli = Cli::parse_from(["gglib", "--verbose", "model", "list"]);
        assert!(cli.verbose);
        assert!(
            Cli::try_parse_from(["gglib", "--models-dir", "/tmp/models", "model", "list"]).is_err()
        );
    }

    #[test]
    fn test_up_parses_its_flags() {
        use clap::Parser;
        let cli = Cli::parse_from(["gglib", "up", "--yes", "--model", "qwen", "--port", "9999"]);
        match cli.command {
            Some(crate::commands::Commands::Up { yes, model, port }) => {
                assert!(yes);
                assert_eq!(model.as_deref(), Some("qwen"));
                assert_eq!(port, Some(9999));
            }
            _ => panic!("expected the Up variant"),
        }
    }

    /// `q` is the command's one name: `question` is no spelling of it.
    #[test]
    fn q_is_the_one_name_of_the_question_command() {
        use clap::Parser;
        let cli = Cli::parse_from(["gglib", "q", "hi"]);
        assert!(matches!(
            cli.command,
            Some(crate::commands::Commands::Question { .. })
        ));
        let err = Cli::try_parse_from(["gglib", "question", "hi"]).err();
        assert_eq!(
            err.map(|e| e.kind()),
            Some(clap::error::ErrorKind::InvalidSubcommand)
        );
    }

    /// A parent's own arguments typed before its subcommand are refused by
    /// name rather than dropped. The bare forms, flags after the subcommand
    /// and the global `--verbose` anywhere still parse.
    #[test]
    fn a_parents_arguments_before_its_subcommand_are_refused() {
        use crate::remote_flag::{Refusal, parse_from};
        let refused = |argv: &[&str]| match parse_from(argv) {
            Err(Refusal::Sentence(sentence)) => sentence,
            Err(Refusal::Clap(error)) => panic!("`{}` got clap's error: {error}", argv.join(" ")),
            Ok(_) => panic!("`{}` parsed", argv.join(" ")),
        };

        assert_eq!(
            refused(&["gglib", "proxy", "--port", "8081", "dashboard"]),
            "`gglib proxy dashboard` takes none of `gglib proxy`'s own arguments, so --port \
             before `dashboard` would be dropped; drop it, or type it after `dashboard` if it \
             takes it"
        );
        assert!(
            refused(&["gglib", "chat", "m", "--no-tools", "history"])
                .contains("so `m` and --no-tools before `history` would be dropped; drop them")
        );
        for argv in [
            &["gglib", "chat", "--no-tools", "history"][..],
            &["gglib", "chat", "--continue", "3", "history"],
            &["gglib", "chat", "--remote", "history"],
            &["gglib", "proxy", "--cache", "stop"],
            &["gglib", "proxy", "--temperature", "0.5", "cache-clear"],
        ] {
            let text = refused(argv);
            assert!(text.contains("would be dropped"), "{text}");
        }

        for argv in [
            &["gglib", "proxy"][..],
            &["gglib", "proxy", "--port", "9"],
            &["gglib", "proxy", "dashboard", "--port", "9"],
            &["gglib", "proxy", "-v", "dashboard"],
            &["gglib", "chat", "m"],
            &["gglib", "chat", "history"],
            &["gglib", "chat", "history", "-n", "3"],
            &["gglib", "chat", "-v", "history"],
            &["gglib", "-v", "chat", "history"],
        ] {
            assert!(parse_from(argv).is_ok(), "{}", argv.join(" "));
        }
    }

    /// `model check-updates` takes its model as every model command does, or
    /// `--all`: exactly one, and clap refuses both or neither.
    #[test]
    fn check_updates_takes_a_model_or_all_and_not_both() {
        use crate::model_commands::ModelCommand;
        use clap::Parser;
        use clap::error::ErrorKind;
        let parsed = |args: &[&str]| {
            let line = [&["gglib", "model", "check-updates"], args].concat();
            match Cli::try_parse_from(line).map(|cli| cli.command) {
                Ok(Some(crate::commands::Commands::Model {
                    command: ModelCommand::CheckUpdates { identifier, all },
                })) => Ok((identifier, all)),
                Ok(_) => panic!("expected check-updates"),
                Err(err) => Err(err.kind()),
            }
        };

        assert_eq!(parsed(&["qwen"]), Ok((Some("qwen".to_owned()), false)));
        assert_eq!(parsed(&["--all"]), Ok((None, true)));
        assert_eq!(parsed(&["qwen", "--all"]), Err(ErrorKind::ArgumentConflict));
        assert_eq!(parsed(&[]), Err(ErrorKind::MissingRequiredArgument));
        assert_eq!(
            parsed(&["--identifier", "qwen"]),
            Err(ErrorKind::UnknownArgument)
        );
    }

    /// No `--port` flag carries a default of its own. A default typed into
    /// clap reaches the handler as a port somebody chose, and on the five
    /// commands that name the proxy's port it would outrank the stored
    /// `proxy_port`; an absent flag has to arrive as absent.
    #[test]
    fn no_port_flag_carries_a_default_of_its_own() {
        fn walk(cmd: &clap::Command, path: &str, seen: &mut Vec<(String, bool)>) {
            for arg in cmd.get_arguments() {
                if arg.get_long() == Some("port") {
                    seen.push((path.to_owned(), !arg.get_default_values().is_empty()));
                }
            }
            for sub in cmd.get_subcommands() {
                walk(sub, &format!("{path} {}", sub.get_name()), seen);
            }
        }

        let mut seen = Vec::new();
        walk(&Cli::command(), "gglib", &mut seen);

        // The five that name the proxy's port. Finding them is what shows the
        // walk reads the tree rather than passing over an empty one.
        for command in [
            "gglib up",
            "gglib serve",
            "gglib proxy",
            "gglib proxy dashboard",
            "gglib proxy cache-clear",
        ] {
            assert!(
                seen.iter().any(|(path, _)| path == command),
                "`{command}` has no --port flag: {seen:?}"
            );
        }
        let defaulted: Vec<&str> = seen
            .iter()
            .filter(|(_, has_default)| *has_default)
            .map(|(path, _)| path.as_str())
            .collect();
        assert!(
            defaulted.is_empty(),
            "--port has a default on {defaulted:?}"
        );
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
