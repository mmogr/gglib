//! Tests for which credential the CLI presents.

use std::path::PathBuf;

use super::*;
use crate::daemon_client::{DaemonProbe, wait_for_launch};

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

fn local(path: &Path) -> Local {
    Local {
        token_path: Some(path.to_owned()),
    }
}

/// The CLI's own credential is this machine's: the token file beside the
/// device keys. Named in this binary's own data root, because naming it
/// makes or tightens `<data root>/data`, otherwise the user's real one
/// (#955).
#[test]
fn here_is_the_daemons_token_file() {
    gglib_core::paths::isolate_data_root();
    assert_eq!(Local::here().token_path, daemon_token_path().ok());
}

#[test]
fn the_token_file_is_read_trimmed() {
    let (_dir, path) = temp();
    write_token(&path, "the-token\n");

    assert_eq!(local(&path).token().as_deref(), Some("the-token"));
}

/// No file, or one that is not a file, as another account's is to this one:
/// unreadable is absent, and nothing else is sent in its place.
#[test]
fn an_unreadable_token_file_is_no_token() {
    let (_dir, path) = temp();
    assert_eq!(local(&path).token(), None);

    std::fs::create_dir(&path).expect("a directory where the file would be");
    assert_eq!(local(&path).token(), None);
}

/// A token file others could read, or have written, is not presented.
#[cfg(unix)]
#[test]
fn a_token_file_open_to_others_is_not_sent() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = temp();
    write_token(&path, "planted");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");

    assert_eq!(local(&path).token(), None);
}

/// A daemon this command launched minted a new token as it started, after the
/// command looked for one; the handle carries the new one.
#[tokio::test]
async fn a_launched_daemon_is_sent_the_token_it_just_minted() {
    let (dir, path) = temp();
    write_token(&path, "minted-at-start");

    let handle = wait_for_launch(
        gglib_proxy::loopback::client(),
        &local(&path),
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
    assert_eq!(
        unauthorized_under(&body, false),
        DAEMON_TOKEN_REQUIRED_MESSAGE
    );
    // The daemon has no other 401, so any other body gets the same words.
    assert_eq!(
        unauthorized_under("not json", false),
        DAEMON_TOKEN_REQUIRED_MESSAGE
    );
}

/// Under `GGLIB_DATA_DIR` a token refusal most likely means the daemon on
/// the port is another data root's, and the message says what to set.
#[test]
fn a_token_refusal_under_its_own_data_root_names_the_daemon_port() {
    let body = serde_json::json!({
        "error": DAEMON_TOKEN_REQUIRED_MESSAGE,
        "type": DAEMON_TOKEN_REQUIRED_TYPE,
    })
    .to_string();
    let said = unauthorized_under(&body, true);
    assert!(said.starts_with(DAEMON_TOKEN_REQUIRED_MESSAGE), "{said}");
    assert!(said.contains("GGLIB_DAEMON_PORT"), "{said}");

    let other = unauthorized_under("not json", true);
    assert!(other.starts_with(DAEMON_TOKEN_REQUIRED_MESSAGE), "{other}");
    assert!(other.contains("GGLIB_DAEMON_PORT"), "{other}");
}
