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

/// The keys `gglib config <command>` lists as values, hidden ones included.
fn values(command: &str) -> Vec<String> {
    let mut cli = <crate::Cli as clap::CommandFactory>::command();
    let config = cli.find_subcommand_mut("config").expect("config");
    let sub = config.find_subcommand_mut(command).expect(command);
    let key = sub
        .get_arguments()
        .find(|arg| arg.get_id() == "key")
        .expect("a key argument");
    key.get_possible_values()
        .iter()
        .map(|value| value.get_name().to_owned())
        .collect()
}

/// `set` lists exactly the keys it writes and `unset` the keys it clears;
/// neither offers one it would only refuse.
#[test]
fn each_command_lists_the_keys_it_changes() {
    let settable: Vec<String> = KEYS
        .iter()
        .filter(|key| key.settable())
        .map(|key| key.name.to_owned())
        .collect();
    let clearable: Vec<String> = KEYS
        .iter()
        .filter(|key| key.clearable())
        .map(|key| key.name.to_owned())
        .collect();
    assert_eq!(values("set"), settable);
    assert_eq!(values("unset"), clearable);
    assert!(!settable.contains(&"inference-profiles".to_owned()));
    assert!(clearable.contains(&"inference-profiles".to_owned()));
    assert!(!clearable.contains(&"remote-enabled".to_owned()));
}

/// `config set --help` names every listed key with what it takes, and ends
/// with each key it does not take and where that one is changed; a key clap
/// does not list is refused with that list rather than run.
#[test]
fn the_help_lists_the_keys_and_an_unknown_key_is_refused_with_them() {
    let Err(help) = crate::Cli::try_parse_from(["gglib", "config", "set", "--help"]) else {
        panic!("--help prints help");
    };
    let help = help.to_string();
    let (listed, elsewhere) = help
        .split_once("Changed elsewhere:")
        .expect("the help names the keys set does not take");
    for key in KEYS.iter().filter(|key| !key.hidden && key.settable()) {
        assert!(
            listed.contains(&format!("{}: ", key.name)),
            "{}: {help}",
            key.name
        );
    }
    assert!(listed.contains("a whole number (>= 1024)"), "{help}");
    for (name, where_) in [
        ("inference-profiles", "edited with `gglib config profile`"),
        (
            "inference-defaults",
            "edited under Settings › Models in the app",
        ),
        (
            "remote-enabled",
            "`gglib remote enable` and `gglib remote disable`",
        ),
        ("remote-pairing", "`gglib remote join` writes it"),
    ] {
        assert!(!listed.contains(name), "{name}: {help}");
        assert!(elsewhere.contains(&format!("{name}: {where_}")), "{help}");
    }

    let Err(refused) =
        crate::Cli::try_parse_from(["gglib", "config", "set", "proxy-loop-detection", "false"])
    else {
        panic!("an unknown key parses");
    };
    assert_eq!(refused.kind(), clap::error::ErrorKind::InvalidValue);
    let said = refused.to_string();
    assert!(said.contains("loop-guard"), "{said}");
}

/// A hidden key the commands change still parses; a key they do not change
/// is refused by clap, whose list it is not on.
#[test]
fn a_hidden_key_parses_and_a_key_changed_elsewhere_does_not() {
    for argv in [
        &["gglib", "config", "set", "setup-completed", "true"][..],
        &["gglib", "config", "unset", "setup-completed"],
        &["gglib", "config", "unset", "inference-profiles"],
    ] {
        crate::Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
    }
    for argv in [
        &["gglib", "config", "set", "remote-enabled", "true"][..],
        &["gglib", "config", "set", "inference-profiles", "x"],
        &["gglib", "config", "set", "inference-defaults", "x"],
        &["gglib", "config", "unset", "remote-pairing"],
    ] {
        let refused = crate::Cli::try_parse_from(argv).err();
        assert_eq!(
            refused.map(|e| e.kind()),
            Some(clap::error::ErrorKind::InvalidValue),
            "{argv:?}"
        );
    }
}
