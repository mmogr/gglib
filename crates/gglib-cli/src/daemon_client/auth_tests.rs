//! Tests for which credential the CLI presents.

use std::path::PathBuf;

use super::*;
use gglib_core::contracts::http::daemon::DAEMON_TOKEN_REQUIRED_MESSAGE;

/// A directory of its own, and the token file's path inside it.
fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("daemon_token");
    (dir, path)
}

#[tokio::test]
async fn the_token_file_is_preferred_to_the_api_key() {
    let (_dir, path) = temp();
    std::fs::write(&path, "the-token\n").expect("write");
    // `0600`, as the daemon writes it: a file open to others is not read.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    }
    let fallback = async { Some("the-key".to_owned()) };

    assert_eq!(
        prefer_token(Some(&path), fallback).await.as_deref(),
        Some("the-token")
    );
}

#[tokio::test]
async fn without_a_token_file_the_api_key_is_sent() {
    let (_dir, path) = temp();
    let fallback = async { Some("the-key".to_owned()) };

    assert_eq!(
        prefer_token(Some(&path), fallback).await.as_deref(),
        Some("the-key")
    );
}

/// Another account's file, or one that is not a file: unreadable is absent.
#[tokio::test]
async fn an_unreadable_token_file_falls_back_to_the_api_key() {
    let (_dir, path) = temp();
    std::fs::create_dir(&path).expect("a directory where the file would be");
    let fallback = async { Some("the-key".to_owned()) };

    assert_eq!(
        prefer_token(Some(&path), fallback).await.as_deref(),
        Some("the-key")
    );
}

#[test]
fn a_trust_routes_401_is_shown_in_the_daemons_words() {
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
