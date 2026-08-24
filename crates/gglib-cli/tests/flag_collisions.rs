//! One meaning per flag.
//!
//! Two ways a flag surface goes wrong without saying so, both of which this
//! one already had:
//!
//! 1. **A subcommand redeclaring a global's arg id.** Clap's propagation step
//!    declines to push a global into a subcommand that already uses that id.
//!    The global then works everywhere except there, and `--help` still reads
//!    plausibly, because the local flag is sitting in the slot the global
//!    would have occupied. `gglib q` and `gglib model verify` each swallowed
//!    `--verbose` this way, which left both with no route to debug logging at
//!    all — not a naming wart, a missing capability.
//!
//! 2. **A short flag that means two things among siblings.** Clap rejects two
//!    `-f`s inside one command and has no opinion whatever about `-f` being
//!    `--force` under one subcommand and `--file` under the next. Nothing else
//!    was looking either.
//!
//! Like the other suites here these read the parsed clap model rather than
//! `--help` text: the flag set is the contract, the rendered help is
//! formatting. They read the *unbuilt* model specifically — see
//! [`descendants`], where the reason lives.

#[path = "support/flag_surface.rs"]
mod flag_surface;

use std::collections::BTreeMap;

use clap::{Command, CommandFactory, Id};
use gglib_cli::Cli;

use flag_surface::descendants;

/// The one short flag that already meant two things among its siblings: `-s`
/// is `--shards` on `gglib model repair` and `--sort` on `gglib model search`.
///
/// Transcribed rather than tolerated in silence, so that the *next* one fails.
/// It is grandfathered instead of renamed because neither spelling shadows
/// anything — no global claims `-s`, and no workflow runs both commands in one
/// breath. The value of the list is that growing it takes a deliberate edit
/// with a reviewer attached.
const GRANDFATHERED_SHORT_REUSE: &[(&str, char)] = &[("gglib model", 's')];

/// A local flag whose id matches a global's suppresses that global on its
/// command, silently and completely.
///
/// The repair is always to rename the *field*, never merely to drop its
/// `short`: the id is what clap matches on, so `#[arg(long)] verbose: bool`
/// keeps the global out just as effectively as `#[arg(short, long)]` did, and
/// the only visible difference is that `-v` stops being advertised as well as
/// stops working.
#[test]
fn no_subcommand_redeclares_a_global_arg_id() {
    let root = Cli::command();
    let globals: Vec<&Id> = root
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .map(clap::Arg::get_id)
        .collect();
    assert!(
        !globals.is_empty(),
        "no `global = true` args found on the root command, so this walk \
         would pass whatever the subcommands declared"
    );

    let shadowed: Vec<String> = descendants(&root)
        .into_iter()
        .flat_map(|(path, cmd)| {
            cmd.get_arguments()
                .filter(|arg| globals.contains(&arg.get_id()))
                .map(|arg| format!("`{path}` declares `{}`", arg.get_id()))
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(
        shadowed.is_empty(),
        "a subcommand may not reuse a global's arg id — the global stops \
         reaching that command entirely:\n  {}\n\
         Rename the local field and its flag. Dropping only the `short` \
         leaves the id in place, and the global still never arrives.",
        shadowed.join("\n  ")
    );
}

/// `-s` should not be `--shards` here and `--sort` there.
///
/// Clap enforces this inside a single command and cannot see across siblings,
/// which is where the confusion actually lands: a user learns a short form
/// from one subcommand and carries it to the next one in the same group.
#[test]
fn a_short_flag_means_one_thing_within_a_command_group() {
    let root = Cli::command();
    let mut groups: Vec<(String, &Command)> = vec![(root.get_name().to_owned(), &root)];
    groups.extend(
        descendants(&root)
            .into_iter()
            .filter(|(_, cmd)| cmd.has_subcommands()),
    );

    let mut divergent = Vec::new();
    for (path, parent) in groups {
        for (short, meanings) in shorts_by_meaning(parent) {
            if meanings.len() < 2 || GRANDFATHERED_SHORT_REUSE.contains(&(path.as_str(), short)) {
                continue;
            }
            let spelled: Vec<String> = meanings
                .iter()
                .map(|(long, subs)| format!("--{long} on `{}`", subs.join("`, `")))
                .collect();
            divergent.push(format!(
                "`{path} …`: -{short} is {}",
                spelled.join(", and ")
            ));
        }
    }

    assert!(
        divergent.is_empty(),
        "these short flags mean more than one thing inside a single command \
         group:\n  {}\n\
         Give the newcomer its own letter, or none — or, if the clash is \
         genuinely harmless, name it in GRANDFATHERED_SHORT_REUSE and say why.",
        divergent.join("\n  ")
    );
}

/// For one parent: every short its immediate children declare, mapped to the
/// long names it stands for and the subcommands that spell it that way.
fn shorts_by_meaning(parent: &Command) -> BTreeMap<char, BTreeMap<String, Vec<String>>> {
    let mut by_short: BTreeMap<char, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for sub in parent.get_subcommands() {
        for arg in sub.get_arguments() {
            let Some(short) = arg.get_short() else {
                continue;
            };
            by_short
                .entry(short)
                .or_default()
                .entry(arg.get_long().unwrap_or("<positional>").to_owned())
                .or_default()
                .push(sub.get_name().to_owned());
        }
    }
    by_short
}
