//! Unit tests for [`super`], over a database of the test's own.

use gglib_core::settings::SettingsError;

use super::*;
use crate::bootstrap::test_context;

/// The table's row for `name`.
pub(in crate::handlers::config::settings) fn key(name: &str) -> SettingKey {
    *crate::config_commands::KEYS
        .iter()
        .find(|key| key.name == name)
        .unwrap_or_else(|| panic!("no key {name}"))
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
            "loop-guard",
            "refuse",
            "`loop-guard` takes true or false, got 'refuse'",
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
    handle_set(&ctx, key("loop-guard"), "true")
        .await
        .expect("seeded");
    handle_set(&ctx, key("max-stagnation-steps"), "7")
        .await
        .expect("seeded");

    handle_set(&ctx, key("loop-guard"), "false")
        .await
        .expect("stored");
    handle_set(&ctx, key("proxy-port"), "9191")
        .await
        .expect("stored");

    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.loop_guard, Some(false));
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
