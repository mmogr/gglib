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
//! 2. **A short flag that means two things.** Clap rejects two `-f`s inside
//!    one command and has no opinion whatever about `-f` being `--force` under
//!    one subcommand and `--file` under the next. Nothing else was looking
//!    either, so every short is now read across the whole tree.
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

/// Every short flag gglib has, and the one long flag each stands for. Any
/// other flag is long only.
const SHORTS: &[(char, &str)] = &[
    ('c', "context-size"),
    ('f', "follow"),
    ('m', "model"),
    ('n', "limit"),
    ('o', "output"),
    ('p', "port"),
    ('q', "quantization"),
    ('s', "system-prompt"),
    ('v', "verbose"),
    ('y', "yes"),
];

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
/// Clap enforces one meaning inside a single command and cannot see across
/// commands, which is where the confusion lands: a user learns a short form
/// from one command and carries it to the next. So the whole tree is read,
/// and each letter must stand for the one long flag [`SHORTS`] gives it.
/// A letter not in the table is refused too: a new short is a decision for
/// the table, not for one command.
#[test]
fn a_short_flag_means_one_thing_everywhere() {
    let root = Cli::command();
    let mut commands: Vec<(String, &Command)> = vec![(root.get_name().to_owned(), &root)];
    commands.extend(descendants(&root));

    let mut found: BTreeMap<char, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for (path, cmd) in commands {
        for arg in cmd.get_arguments() {
            let Some(short) = arg.get_short() else {
                continue;
            };
            found
                .entry(short)
                .or_default()
                .entry(arg.get_long().unwrap_or("<positional>").to_owned())
                .or_default()
                .push(path.clone());
        }
    }

    let wrong: Vec<String> = found
        .iter()
        .flat_map(|(short, meanings)| {
            let meant = SHORTS
                .iter()
                .find(|(s, _)| s == short)
                .map(|(_, long)| *long);
            meanings
                .iter()
                .filter(move |(long, _)| Some(long.as_str()) != meant)
                .map(move |(long, paths)| {
                    format!("-{short} is --{long} on `{}`", paths.join("`, `"))
                })
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "these short flags are not the ones the table gives:\n  {}\n\
         A short flag means one long flag everywhere. Drop the short, or \
         give the long the table's name; a new letter goes in SHORTS.",
        wrong.join("\n  ")
    );

    let unused: Vec<String> = SHORTS
        .iter()
        .filter(|(short, _)| !found.contains_key(short))
        .map(|(short, long)| format!("-{short} (--{long})"))
        .collect();
    assert!(unused.is_empty(), "no command has {}", unused.join(", "));
}
