//! `--remote`, declared on each command that reaches the paired machine
//! (ADR 0028), and what the command line is told when it is typed anywhere
//! else.
//!
//! clap refuses the flag on a command that does not declare it, as an
//! unknown argument. [`parse_from`] turns that into ADR 0013's sentence: it
//! parses the command line again without the flag and asks
//! [`Target::admit`] about the command it finds. A command that declares
//! the flag but had it typed before its name is told where it goes. Which
//! machine a command is for is read from the parse, on whichever command
//! declared the flag, so no list here can disagree with the parser.

use std::ffi::OsString;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{ArgMatches, Args, CommandFactory as _, FromArgMatches as _};

use crate::parser::Cli;
use crate::target::{Reach, Target, reach};

/// The id clap knows the flag by, on every command that declares it.
const REMOTE: &str = "remote";

/// `--remote`, flattened into each command that reaches the paired machine.
///
/// The field is the declaration; [`parse_from`] reads whether it was typed
/// from the parse itself.
#[derive(Args, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoteFlag {
    /// Do this on the machine this one is paired with (`gglib remote status` names it)
    #[arg(long)]
    pub remote: bool,
}

/// Why a command line did not parse.
#[derive(Debug)]
pub(crate) enum Refusal {
    /// clap's own error, which it prints and exits on.
    Clap(clap::Error),
    /// `--remote` on a command that does not take it, or before the command
    /// that does: one sentence.
    Remote(String),
}

/// Parse the command line, or exit as clap does; `--remote` where it does
/// not belong exits with one sentence instead of clap's error.
///
/// Returns the parse and whether the command is for the paired machine.
#[must_use]
pub fn parse() -> (Cli, bool) {
    match parse_from(std::env::args_os()) {
        Ok(parsed) => parsed,
        Err(Refusal::Clap(error)) => error.exit(),
        Err(Refusal::Remote(sentence)) => {
            eprintln!("Error: {sentence}");
            std::process::exit(1);
        }
    }
}

/// [`parse`], from `argv`, without exiting.
pub(crate) fn parse_from<I, T>(argv: I) -> Result<(Cli, bool), Refusal>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let argv: Vec<OsString> = argv.into_iter().map(Into::into).collect();
    let mut command = Cli::command();
    match command.try_get_matches_from_mut(&argv) {
        Ok(mut matches) => {
            let remote = typed(&matches);
            let cli = Cli::from_arg_matches_mut(&mut matches)
                .map_err(|error| Refusal::Clap(error.format(&mut command)))?;
            Ok((cli, remote))
        }
        Err(error) if names_remote(&error) => {
            Err(misplaced(&argv).map_or(Refusal::Clap(error), Refusal::Remote))
        }
        Err(error) => Err(Refusal::Clap(error)),
    }
}

/// Each command that declares `--remote`, named as `--help` names it
/// (`model list`), in the order `--help` lists them.
pub(crate) fn declared_on() -> Vec<String> {
    fn walk(command: &clap::Command, path: &str, found: &mut Vec<String>) {
        let mut subcommands: Vec<&clap::Command> = command.get_subcommands().collect();
        subcommands.sort_by_key(|sub| sub.get_display_order());
        for sub in subcommands {
            let here = if path.is_empty() {
                sub.get_name().to_owned()
            } else {
                format!("{path} {}", sub.get_name())
            };
            if sub.get_arguments().any(|arg| arg.get_id() == REMOTE) {
                found.push(here.clone());
            }
            walk(sub, &here, found);
        }
    }
    let mut found = Vec::new();
    walk(&Cli::command(), "", &mut found);
    found
}

/// Whether `--remote` was typed on any command in the parse that declares it.
fn typed(matches: &ArgMatches) -> bool {
    let mut level = Some(matches);
    while let Some(here) = level {
        if matches!(here.try_get_one::<bool>(REMOTE), Ok(Some(true))) {
            return true;
        }
        level = here.subcommand().map(|(_, sub)| sub);
    }
    false
}

/// Whether clap refused `--remote` itself, as an argument the command does
/// not have.
fn names_remote(error: &clap::Error) -> bool {
    error.kind() == ErrorKind::UnknownArgument
        && matches!(
            error.get(ContextKind::InvalidArg),
            Some(ContextValue::String(arg)) if arg == "--remote"
        )
}

/// The sentence for `--remote` where clap refused it: the refusal of a
/// command that is about this machine, or where the flag goes on one that
/// takes it. `None` when the line names no command, or does not parse
/// without the flag either, so clap's own error is the clearer one.
fn misplaced(argv: &[OsString]) -> Option<String> {
    let without: Vec<&OsString> = argv.iter().filter(|arg| *arg != "--remote").collect();
    let cli = Cli::command()
        .try_get_matches_from(without)
        .ok()
        .and_then(|mut matches| Cli::from_arg_matches_mut(&mut matches).ok())?;
    let command = cli.command?;
    match reach(&command) {
        (name, Reach::Use) => Some(format!(
            "--remote goes after the command it is for: `gglib {name} … --remote`"
        )),
        _ => Target::Remote.admit(&command).err().map(|e| e.to_string()),
    }
}

#[cfg(test)]
#[path = "remote_flag_tests.rs"]
mod remote_flag_tests;
