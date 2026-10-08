//! Unit tests for [`super`], over a database of the test's own.

use clap::{Args, Command, FromArgMatches, Parser};
use gglib_core::LoopGuardMode;
use gglib_core::settings::SettingsError;

use super::*;
use crate::bootstrap::test_context;

/// The arguments clap hands the handler for `flags`, if they parse.
fn parsed(flags: &[&str]) -> Option<SettingsSetArgs> {
    let matches = SettingsSetArgs::augment_args(Command::new("set"))
        .try_get_matches_from(std::iter::once("set").chain(flags.iter().copied()))
        .ok()?;
    SettingsSetArgs::from_arg_matches(&matches).ok()
}

fn args(flags: &[&str]) -> SettingsSetArgs {
    parsed(flags).expect("the flags parse")
}

/// The flag list is clap's own, so a flag added to `SettingsSetArgs` and left
/// out of `update_from` is caught here: it would write nothing and print
/// nothing.
#[test]
fn every_flag_clap_accepts_is_a_write_reported_under_the_flags_own_name() {
    let command = SettingsSetArgs::augment_args(Command::new("set"));
    let flags: Vec<String> = command
        .get_arguments()
        .filter_map(|arg| arg.get_long().map(str::to_owned))
        .collect();
    assert!(flags.len() >= 18, "expected every settable flag: {flags:?}");

    for flag in flags {
        let long = format!("--{flag}");
        let given = ["true", "8192", "refuse"]
            .into_iter()
            .find_map(|value| parsed(&[&long, value]))
            .unwrap_or_else(|| panic!("{long} takes none of the sample values"));

        let changed = changed_keys(&update_from(given)).expect("keys");

        assert_eq!(changed, BTreeSet::from([flag]), "{long}");
    }
    let nothing = changed_keys(&update_from(args(&[]))).expect("keys");
    assert!(nothing.is_empty(), "no flag, no write: {nothing:?}");
}

/// The refusal is the validator's own sentence with nothing put around it,
/// and nothing is stored, not even the valid flag given beside the bad one.
#[tokio::test]
async fn a_refused_value_is_reported_in_the_validators_words_and_stores_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let before = ctx.app.settings().get().await.expect("settings");

    let refused = handle_set(
        &ctx,
        args(&["--proxy-port", "80", "--default-context-size", "8192"]),
    )
    .await
    .expect_err("a privileged port is refused");

    assert_eq!(
        format!("{refused:#}"),
        SettingsError::InvalidPort(80).to_string()
    );
    let after = ctx.app.settings().get().await.expect("settings");
    assert_eq!(after, before);
}

/// What is stored is the service's own merge of the flags onto the record as
/// it stood: the fields named are written, and one set earlier and not named
/// now is left as it was.
#[tokio::test]
async fn a_valid_value_stores_what_the_services_merge_produces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    handle_set(
        &ctx,
        args(&["--loop-guard-mode", "off", "--max-stagnation-steps", "7"]),
    )
    .await
    .expect("seeded");
    let flags = ["--loop-guard-mode", "refuse", "--proxy-port", "9191"];
    let mut expected = ctx.app.settings().get().await.expect("settings");
    assert_eq!(expected.loop_guard_mode, Some(LoopGuardMode::Off));
    expected.merge(&update_from(args(&flags)));

    handle_set(&ctx, args(&flags)).await.expect("stored");

    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored, expected);
    assert_eq!(stored.loop_guard_mode, Some(LoopGuardMode::Refuse));
    assert_eq!(stored.proxy_port, Some(9191));
    assert_eq!(stored.max_stagnation_steps, Some(7));
}

/// `--proxy-loop-detection` was the loop guard's switch before
/// `--loop-guard-mode`, and is a flag no longer. A script that still passes
/// it is refused by clap as an unknown argument before anything is read or
/// stored, so the guard is never left on in silence. The second parse shows
/// the refusal is of that flag and not of the command around it.
#[test]
fn the_retired_loop_detection_flag_is_refused_as_an_unknown_argument() {
    let set = ["gglib", "config", "settings", "set"];

    let Err(refused) =
        crate::Cli::try_parse_from(set.into_iter().chain(["--proxy-loop-detection", "false"]))
    else {
        panic!("the retired flag still parses");
    };

    assert_eq!(refused.kind(), clap::error::ErrorKind::UnknownArgument);
    crate::Cli::try_parse_from(set.into_iter().chain(["--loop-guard-mode", "off"]))
        .expect("the flag that replaced it parses on the same command");
}
