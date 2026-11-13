//! The command line and settings keys `docs/compatibility.md` promises,
//! held to a checked-in list.
//!
//! Every other guard here derives what it expects from the code it guards, so
//! a rename passes them all. This one compares the clap tree and the
//! `Settings` keys against `promised_surface.txt`: a command, flag, value or
//! key added shows as a new line there, and one renamed or removed as a
//! deleted line, which is the change a reviewer has to see. Rewrite the file
//! with `GGLIB_WRITE_PROMISED_SURFACE=1 cargo test -p gglib-cli --test
//! promised_surface`.
//!
//! A line marked `(not covered)` is one the page lists under "Not covered"
//! (benchmarks, `model explain`, the proxy dashboard, `llama_base_port`, the
//! loop guard), so deleting it breaks nothing. Values a flag checks with a
//! function of its own, such as `--reasoning-effort`'s levels, are not clap's
//! to list and are not here.
//!
//! Reads the unbuilt command, as `support/flag_surface.rs` explains, so a
//! global flag is listed once, on `gglib`, and clap's own `--help` and
//! `--version` are not listed.

use std::fmt::Write as _;
use std::path::Path;

use clap::{Arg, Command, CommandFactory};
use gglib_cli::Cli;
use gglib_core::Settings;

const LIST: &str = "tests/promised_surface.txt";

const HEADER: &str = "\
# The command line and settings keys docs/compatibility.md promises. A line
# marked (not covered) is one the page lists under Not covered.
# Rewrite with: GGLIB_WRITE_PROMISED_SURFACE=1 cargo test -p gglib-cli --test promised_surface
";

/// The commands and keys `docs/compatibility.md` lists under "Not covered":
/// a command with everything under it, a key by its `config set` and its
/// stored name.
const NOT_COVERED_COMMANDS: &[&str] = &[
    "gglib benchmark",
    "gglib model explain",
    "gglib proxy dashboard",
];
const NOT_COVERED_KEYS: &[&str] = &[
    "llama-base-port",
    "llama_base_port",
    "loop-guard",
    "loop_guard",
];

fn command_covered(path: &str) -> bool {
    !NOT_COVERED_COMMANDS
        .iter()
        .any(|out| path == *out || path.starts_with(&format!("{out} ")))
}

/// ` (hidden)` and ` (not covered)`, as they apply.
fn marks(hidden: bool, covered: bool) -> String {
    let mut marks = String::new();
    if hidden {
        marks.push_str(" (hidden)");
    }
    if !covered {
        marks.push_str(" (not covered)");
    }
    marks
}

/// One flag or valued argument: its names, the values it takes, and its
/// marks.
fn arg_line(arg: &Arg, covered: bool) -> Option<String> {
    let mut names = Vec::new();
    if let Some(long) = arg.get_long() {
        names.push(format!("--{long}"));
    }
    if let Some(short) = arg.get_short() {
        names.push(format!("-{short}"));
    }
    for alias in arg.get_all_aliases().unwrap_or_default() {
        names.push(format!("--{alias}"));
    }
    for alias in arg.get_all_short_aliases().unwrap_or_default() {
        names.push(format!("-{alias}"));
    }
    // A switch reports `true|false`, which it never takes on the command line.
    let values: Vec<String> = if arg.get_action().takes_values() {
        arg.get_possible_values()
            .iter()
            .flat_map(|value| {
                value.get_name_and_aliases().map(move |name| {
                    let key_covered = !NOT_COVERED_KEYS.contains(&name);
                    format!("{name}{}", marks(value.is_hide_set(), key_covered))
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    if names.is_empty() {
        // A positional is a value, not a name, unless it takes only some.
        if values.is_empty() {
            return None;
        }
        names.push(format!("<{}>", arg.get_id()));
    }
    let mut line = format!("  {}{}", names.join(" "), marks(arg.is_hide_set(), covered));
    // One value a line, so a renamed value or `config set` key is a line of
    // the diff.
    for value in values {
        let _ = write!(line, "\n    {value}");
    }
    Some(line)
}

/// `cmd` at `path`, its arguments, then each subcommand, in name order.
fn command(cmd: &Command, path: &str, out: &mut String) {
    out.push_str(path);
    let aliases: Vec<&str> = cmd.get_all_aliases().collect();
    if !aliases.is_empty() {
        let _ = write!(out, " (alias {})", aliases.join(", "));
    }
    let covered = command_covered(path);
    out.push_str(&marks(cmd.is_hide_set(), covered));
    out.push('\n');
    let mut args: Vec<String> = cmd
        .get_arguments()
        .filter_map(|arg| arg_line(arg, covered))
        .collect();
    args.sort();
    for line in args {
        out.push_str(&line);
        out.push('\n');
    }
    let mut subs: Vec<&Command> = cmd.get_subcommands().collect();
    subs.sort_by_key(|sub| sub.get_name());
    for sub in subs {
        command(sub, &format!("{path} {}", sub.get_name()), out);
    }
}

/// The keys `settings_kv` stores, one row per `Settings` field by its serde
/// name.
fn stored_keys() -> Vec<String> {
    let serde_json::Value::Object(fields) =
        serde_json::to_value(Settings::default()).expect("settings serialise")
    else {
        panic!("Settings serialises to an object");
    };
    fields.keys().cloned().collect()
}

fn render() -> String {
    let mut out = String::from(HEADER);
    out.push_str("\n[commands]\n");
    command(&Cli::command(), "gglib", &mut out);
    out.push_str("\n[stored settings keys]\n");
    for key in stored_keys() {
        out.push_str(&key);
        out.push_str(&marks(false, !NOT_COVERED_KEYS.contains(&key.as_str())));
        out.push('\n');
    }
    out
}

#[test]
fn the_promised_surface_is_the_checked_in_list() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(LIST);
    let expected = render();
    if std::env::var_os("GGLIB_WRITE_PROMISED_SURFACE").is_some() {
        std::fs::write(&path, &expected).expect("write the list");
    }
    let listed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        listed == expected,
        "the CLI or the settings keys differ from {LIST}. A renamed or removed \
         line not marked (not covered) breaks docs/compatibility.md's promise; \
         an addition is fine. Rewrite \
         it with GGLIB_WRITE_PROMISED_SURFACE=1 cargo test -p gglib-cli --test \
         promised_surface and review the diff"
    );
}

/// The list has to hold something, or a walk that found nothing would match
/// an empty file.
#[test]
fn the_walk_finds_commands_flags_and_keys() {
    let rendered = render();
    for expected in [
        "\ngglib config set\n",
        "  --context-size -c\n",
        "\nproxy_port\n",
        "\n    proxy-port\n",
        "\ngglib benchmark (not covered)\n",
        "\nloop_guard (not covered)\n",
    ] {
        assert!(
            rendered.contains(expected),
            "{expected:?} missing:\n{rendered}"
        );
    }
}
