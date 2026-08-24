//! Reading gglib's own clap model, so a guard can compare it against itself.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.
//!
//! Every function here reads the *parsed* command model rather than `--help`
//! text: the flag set is the contract, the rendered help is formatting.

#![allow(dead_code)]

use clap::{Args, Command, CommandFactory};
use gglib_cli::{Cli, SamplingArgs};

/// Every long flag on a top-level subcommand, as clap resolved it.
pub(crate) fn long_flags(subcommand: &str) -> Vec<String> {
    sorted_long_flags(find(&Cli::command(), subcommand))
}

/// Every long flag on a nested subcommand — `long_flags_at(&["model",
/// "update"])` is `gglib model update`.
pub(crate) fn long_flags_at(path: &[&str]) -> Vec<String> {
    let root = Cli::command();
    let mut cmd = &root;
    for name in path {
        cmd = find(cmd, name);
    }
    sorted_long_flags(cmd)
}

/// The long flags [`SamplingArgs`] contributes, asked of clap rather than
/// transcribed.
///
/// This is the point of the module. A literal list is a second copy of the
/// struct that nothing keeps in step: the one it replaced named seven of the
/// fifteen flags, and because every assertion over it is a `contains`, it
/// passed while covering under half the surface it claimed to guard. Derived,
/// the expectation grows the moment the struct does — a new sampling flag that
/// reaches only one of two commands now fails on the day it is added.
pub(crate) fn sampling_flags() -> Vec<String> {
    sorted_long_flags(&SamplingArgs::augment_args(Command::new("probe")))
}

/// Assert both `serve` and `proxy` carry every flag in `expected`.
///
/// Shared because the two suites that guard this surface — flag parity and
/// the sampling-flag derivation — both need it, and a second copy would be a
/// second thing to keep in step.
pub(crate) fn assert_both_expose(expected: &[String], what: &str) {
    let serve = long_flags("serve");
    let proxy = long_flags("proxy");

    for flag in expected {
        assert!(serve.contains(flag), "`serve` is missing --{flag} ({what})");
        assert!(proxy.contains(flag), "`proxy` is missing --{flag} ({what})");
    }
}

/// A transcribed flag list as owned strings, for comparison against a derived
/// one.
pub(crate) fn owned(flags: &[&str]) -> Vec<String> {
    flags.iter().map(|f| (*f).to_owned()).collect()
}

fn find<'a>(parent: &'a Command, name: &str) -> &'a Command {
    parent
        .get_subcommands()
        .find(|c| c.get_name() == name)
        .unwrap_or_else(|| {
            panic!(
                "no `{name}` subcommand under `{}`",
                parent.get_name().to_owned()
            )
        })
}

fn sorted_long_flags(cmd: &Command) -> Vec<String> {
    let mut flags: Vec<String> = cmd
        .get_arguments()
        .filter_map(|a| a.get_long().map(str::to_owned))
        .collect();
    flags.sort();
    flags
}

/// Every subcommand in the tree, paired with the path that reaches it —
/// `("gglib model verify", &cmd)`.
///
/// Reads the *unbuilt* command. `Command::build()` would first propagate every
/// `global = true` arg down into the subcommands, at which point a locally
/// declared clash and an inherited global are indistinguishable — and the
/// clash is precisely what a caller here wants to see.
pub(crate) fn descendants(root: &Command) -> Vec<(String, &Command)> {
    let mut out = Vec::new();
    collect(root, root.get_name(), &mut out);
    out
}

fn collect<'a>(cmd: &'a Command, path: &str, out: &mut Vec<(String, &'a Command)>) {
    for sub in cmd.get_subcommands() {
        let sub_path = format!("{path} {}", sub.get_name());
        collect(sub, &sub_path, out);
        out.push((sub_path, sub));
    }
}
