//! Unit tests for [`super`]: the help each key shows, and the words clap
//! takes for it.

use clap::Parser;
use gglib_core::domain::agent::MIN_TOOL_TIMEOUT_MS;
use gglib_core::settings::{CONTEXT_SIZE_RANGE, DOWNLOAD_QUEUE_RANGE, MIN_CACHE_DISK_GB, MIN_PORT};
use gglib_core::{MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING};

use super::{KEYS, SettingKey};

fn named(name: &str) -> SettingKey {
    *KEYS
        .iter()
        .find(|key| key.name == name)
        .unwrap_or_else(|| panic!("no key {name}"))
}

/// The range in a key's help must be the range the backend holds the
/// setting to.
///
/// `validate_settings` and what it refuses with derive from the constants
/// in `gglib-core`'s `settings_bounds.rs`, and so does the table; this keeps
/// a literal written into the table from disagreeing in silence. The agent
/// limits are held to the ceiling they are clamped to, since nothing refuses
/// them at save.
#[test]
fn each_bounded_keys_help_states_the_range_the_backend_holds_it_to() {
    fn closed<T: std::fmt::Display>(range: &std::ops::RangeInclusive<T>) -> String {
        format!("({}-{})", range.start(), range.end())
    }
    for (key, stated) in [
        ("default-context-size", closed(&CONTEXT_SIZE_RANGE)),
        ("proxy-port", format!("(>= {MIN_PORT})")),
        ("llama-base-port", format!("(>= {MIN_PORT})")),
        ("max-download-queue-size", closed(&DOWNLOAD_QUEUE_RANGE)),
        ("proxy-cache-disk-gb", format!("(>= {MIN_CACHE_DISK_GB})")),
        (
            "max-tool-iterations",
            format!("(clamped to 1-{MAX_ITERATIONS_CEILING} "),
        ),
        (
            "max-parallel-tools",
            format!("(clamped to 1-{MAX_PARALLEL_TOOLS_CEILING} "),
        ),
        (
            "tool-timeout-ms",
            format!("(clamped to {MIN_TOOL_TIMEOUT_MS}-{MAX_TOOL_TIMEOUT_MS_CEILING} "),
        ),
    ] {
        let help = named(key).help();
        assert!(
            help.contains(&stated),
            "{key} help must state {stated}; it says: {help}"
        );
    }
}

/// `config set --help` names every listed key with what it takes, and a key
/// clap does not know is refused with that list rather than run.
#[test]
fn the_help_lists_the_keys_and_an_unknown_key_is_refused_with_them() {
    let Err(help) = crate::Cli::try_parse_from(["gglib", "config", "set", "--help"]) else {
        panic!("--help prints help");
    };
    let help = help.to_string();
    for key in KEYS.iter().filter(|key| !key.hidden) {
        assert!(
            help.contains(&format!("{}: ", key.name)),
            "{}: {help}",
            key.name
        );
    }
    assert!(help.contains("a whole number (>= 1024)"), "{help}");
    assert!(!help.contains("remote-enabled"), "{help}");

    let Err(refused) =
        crate::Cli::try_parse_from(["gglib", "config", "set", "proxy-loop-detection", "false"])
    else {
        panic!("an unknown key parses");
    };
    assert_eq!(refused.kind(), clap::error::ErrorKind::InvalidValue);
    let said = refused.to_string();
    assert!(said.contains("loop-guard"), "{said}");
}

/// A key `config show` prints and another command writes still parses, so
/// `set` can say which command that is instead of calling it unknown.
#[test]
fn a_hidden_key_still_parses() {
    for argv in [
        &["gglib", "config", "set", "remote-enabled", "true"][..],
        &["gglib", "config", "unset", "remote-pairing"],
        &["gglib", "config", "unset", "setup-completed"],
    ] {
        crate::Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
    }
}
