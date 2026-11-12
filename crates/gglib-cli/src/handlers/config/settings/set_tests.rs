//! Unit tests for [`super`], over a database of the test's own.

use clap::ValueEnum;
use gglib_core::LoopGuardMode;
use gglib_core::settings::SettingsError;

use super::super::unset::handle_unset;
use super::*;
use crate::bootstrap::test_context;

/// The key `config set` parses `name` as.
pub(in crate::handlers::config::settings) fn key(name: &str) -> SettingKey {
    SettingKey::from_str(name, false).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The refusal is the validator's own sentence with nothing put around it,
/// and nothing is stored.
#[tokio::test]
async fn a_refused_value_is_reported_in_the_validators_words_and_stores_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let before = ctx.app.settings().get().await.expect("settings");

    let refused = handle_set(&ctx, key("proxy-port"), "80")
        .await
        .expect_err("a privileged port is refused");

    assert_eq!(
        format!("{refused:#}"),
        SettingsError::InvalidPort(80).to_string()
    );
    assert_eq!(ctx.app.settings().get().await.expect("settings"), before);
}

/// A value of the wrong kind is refused in one sentence that names the key
/// and what it takes, whether the kind is wrong or the field cannot hold
/// it, and nothing is stored.
#[tokio::test]
async fn a_value_the_key_does_not_take_is_refused_saying_what_it_takes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let before = ctx.app.settings().get().await.expect("settings");

    for (name, typed, said) in [
        (
            "proxy-port",
            "abc",
            "`proxy-port` takes a whole number (>= 1024), got 'abc'",
        ),
        (
            "proxy-port",
            "70000",
            "`proxy-port` takes a whole number (>= 1024), got '70000'",
        ),
        (
            "tool-timeout-ms",
            "-1",
            "`tool-timeout-ms` takes a whole number (clamped to 100-60000 when a loop runs), got '-1'",
        ),
        (
            "mcp-drawing",
            "maybe",
            "`mcp-drawing` takes true or false, got 'maybe'",
        ),
        (
            "loop-guard-mode",
            "warn",
            "`loop-guard-mode` takes off, note or refuse, got 'warn'",
        ),
    ] {
        let refused = handle_set(&ctx, key(name), typed)
            .await
            .expect_err("refused");
        assert_eq!(format!("{refused:#}"), said);
    }
    assert_eq!(ctx.app.settings().get().await.expect("settings"), before);
}

/// What is stored is the service's own merge of the one key onto the
/// record as it stood: one set earlier is left as it was.
#[tokio::test]
async fn a_valid_value_is_stored_and_the_others_are_left_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    handle_set(&ctx, key("loop-guard-mode"), "off")
        .await
        .expect("seeded");
    handle_set(&ctx, key("max-stagnation-steps"), "7")
        .await
        .expect("seeded");

    handle_set(&ctx, key("loop-guard-mode"), "refuse")
        .await
        .expect("stored");
    handle_set(&ctx, key("proxy-port"), "9191")
        .await
        .expect("stored");

    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.loop_guard_mode, Some(LoopGuardMode::Refuse));
    assert_eq!(stored.proxy_port, Some(9191));
    assert_eq!(stored.max_stagnation_steps, Some(7));
}

/// `proxy-allowed-hosts a, b` is two hosts, as the app's comma field reads
/// it; the list given replaces the stored one.
#[tokio::test]
async fn the_allowed_hosts_split_on_commas_and_replace_the_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    handle_set(&ctx, key("proxy-allowed-hosts"), "old.lan")
        .await
        .expect("seeded");

    handle_set(
        &ctx,
        key("proxy-allowed-hosts"),
        "gglib.lan, 192.168.1.20:8080,",
    )
    .await
    .expect("stored");

    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(
        stored.proxy_allowed_hosts.as_deref(),
        Some(&["gglib.lan".to_owned(), "192.168.1.20:8080".to_owned()][..])
    );
}

/// A key whose value is a whole record is not set from one argument; the
/// refusal says where it is edited, and `unset` still clears it.
#[tokio::test]
async fn a_whole_record_is_refused_with_where_it_is_edited_and_unset_clears_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;

    let refused = handle_set(&ctx, key("inference-profiles"), "coding")
        .await
        .expect_err("not one value");
    assert_eq!(
        format!("{refused:#}"),
        "`inference-profiles` is not set from one value: it is edited with `gglib config profile`"
    );
    let refused = handle_set(&ctx, key("inference-defaults"), "{}")
        .await
        .expect_err("not one value");
    assert!(
        format!("{refused:#}").contains("Settings › Models"),
        "{refused:#}"
    );

    handle_unset(&ctx, key("inference-profiles"))
        .await
        .expect("cleared");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.inference_profiles, None);
}

/// The remote fields `config show` prints are written by `gglib remote`;
/// `set` and `unset` name the command rather than call the key unknown, and
/// store nothing.
#[tokio::test]
async fn a_remote_field_is_refused_by_set_and_unset_naming_its_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let before = ctx.app.settings().get().await.expect("settings");

    let refused = handle_set(&ctx, key("remote-enabled"), "true")
        .await
        .expect_err("not by hand");
    assert_eq!(
        format!("{refused:#}"),
        "`remote-enabled` is not set by hand: `gglib remote enable` and `gglib remote disable` switch it"
    );
    let refused = handle_unset(&ctx, key("remote-pairing"))
        .await
        .expect_err("not by hand");
    assert!(
        format!("{refused:#}").contains("`gglib remote join` writes it"),
        "{refused:#}"
    );
    assert_eq!(ctx.app.settings().get().await.expect("settings"), before);
}
