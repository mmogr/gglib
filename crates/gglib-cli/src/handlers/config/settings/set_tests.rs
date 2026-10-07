//! Unit tests for [`super`], over a database of the test's own.

use clap::{Args, Command, FromArgMatches};
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
    assert!(flags.len() >= 19, "expected every settable flag: {flags:?}");

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

/// What is stored is the service's own merge of the flags, down to the part
/// a merge written out by hand left out: writing one spelling of the loop
/// guard clears the other.
#[tokio::test]
async fn a_valid_value_stores_what_the_services_merge_produces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    handle_set(&ctx, args(&["--proxy-loop-detection", "false"]))
        .await
        .expect("seeded");
    let flags = ["--loop-guard-mode", "refuse", "--proxy-port", "9191"];
    let mut expected = ctx.app.settings().get().await.expect("settings");
    assert_eq!(expected.proxy_loop_detection, Some(false));
    expected.merge(&update_from(args(&flags)));

    handle_set(&ctx, args(&flags)).await.expect("stored");

    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored, expected);
    assert_eq!(stored.loop_guard_mode, Some(LoopGuardMode::Refuse));
    assert_eq!(stored.proxy_loop_detection, None);
    assert_eq!(stored.proxy_port, Some(9191));
}
