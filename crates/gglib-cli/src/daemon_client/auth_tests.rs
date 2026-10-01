//! Tests for which credential the CLI presents.

use std::path::PathBuf;

use super::*;
use crate::daemon_client::{DaemonProbe, wait_for_launch};
use gglib_core::contracts::http::daemon::DAEMON_TOKEN_REQUIRED_MESSAGE;

/// A directory of its own, and the token file's path inside it.
fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("daemon_token");
    (dir, path)
}

/// Write `token` as the daemon does: `0600`.
fn write_token(path: &Path, token: &str) {
    std::fs::write(path, token).expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    }
}

fn local(env: Option<&str>, path: &Path) -> Local {
    Local {
        env: env.map(str::to_owned),
        token_path: Some(path.to_owned()),
    }
}

fn stored() -> std::future::Ready<Option<String>> {
    std::future::ready(Some("the-key".to_owned()))
}

/// The CLI's own credentials are this machine's: its environment and the
/// token file beside the device keys. Named in this binary's own data root,
/// because naming it makes or tightens `<data root>/data`, in a debug build
/// the checkout's (#955).
#[test]
fn here_is_the_daemons_token_file() {
    gglib_core::paths::isolate_data_root();
    assert_eq!(Local::here().token_path, daemon_token_path().ok());
}

#[tokio::test]
async fn the_token_file_is_preferred_to_the_stored_key() {
    let (_dir, path) = temp();
    write_token(&path, "the-token\n");

    let sent = local(None, &path).credential(stored()).await;
    assert_eq!(sent.as_deref(), Some("the-token"));
}

/// `GGLIB_API_KEY` is the proxy's key, often in `.env`, and a loopback daemon
/// takes only the token, which opens a `--share-lan` daemon too.
#[tokio::test]
async fn the_token_file_is_preferred_to_the_operators_key() {
    let (_dir, path) = temp();
    write_token(&path, "the-token");

    let sent = local(Some("the-env-key"), &path).credential(stored()).await;
    assert_eq!(sent.as_deref(), Some("the-token"));
}

/// Without a token to read, the operator's key outranks the stored one.
#[tokio::test]
async fn without_a_token_file_the_operators_key_is_preferred_to_the_stored_one() {
    let (_dir, path) = temp();

    let sent = local(Some("the-env-key"), &path).credential(stored()).await;
    assert_eq!(sent.as_deref(), Some("the-env-key"));
}

#[tokio::test]
async fn without_a_token_file_the_stored_key_is_sent() {
    let (_dir, path) = temp();

    let sent = local(None, &path).credential(stored()).await;
    assert_eq!(sent.as_deref(), Some("the-key"));
}

/// Another account's file, or one that is not a file: unreadable is absent.
#[tokio::test]
async fn an_unreadable_token_file_falls_back_to_the_stored_key() {
    let (_dir, path) = temp();
    std::fs::create_dir(&path).expect("a directory where the file would be");

    let sent = local(None, &path).credential(stored()).await;
    assert_eq!(sent.as_deref(), Some("the-key"));
}

/// A token file others could read, or have written, is not presented.
#[cfg(unix)]
#[tokio::test]
async fn a_token_file_open_to_others_is_not_sent() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = temp();
    write_token(&path, "planted");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");

    let sent = local(None, &path).credential(stored()).await;
    assert_eq!(sent.as_deref(), Some("the-key"));
}

/// A daemon this command launched minted a new token as it started, after the
/// credential the command resolved; the handle carries the new one.
#[tokio::test]
async fn a_launched_daemon_is_sent_the_token_it_just_minted() {
    let (dir, path) = temp();
    write_token(&path, "minted-at-start");

    let handle = wait_for_launch(
        gglib_proxy::loopback::client(),
        Some("resolved-before-the-launch".to_owned()),
        &local(None, &path),
        dir.path(),
        || async { DaemonProbe::Running },
    )
    .await
    .expect("the probe says it is up");

    assert_eq!(handle.api_key.as_deref(), Some("minted-at-start"));
}

#[test]
fn a_token_refusal_is_shown_in_the_daemons_words() {
    let body = serde_json::json!({
        "error": DAEMON_TOKEN_REQUIRED_MESSAGE,
        "type": DAEMON_TOKEN_REQUIRED_TYPE,
    })
    .to_string();
    assert_eq!(unauthorized(&body), DAEMON_TOKEN_REQUIRED_MESSAGE);

    let key = r#"{"error":"Missing or invalid API key.","type":"INVALID_API_KEY"}"#;
    assert_eq!(unauthorized(key), unauthorized_hint());
    assert_eq!(unauthorized("not json"), unauthorized_hint());
}
