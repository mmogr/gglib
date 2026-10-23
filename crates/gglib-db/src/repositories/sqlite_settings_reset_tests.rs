//! A reset over a stored record this build cannot read, against a database
//! file in a temporary directory: what is dropped, what is kept, and that
//! the kept rows are not touched.

use gglib_core::{Device, RemotePairing, RemoteServe};

use super::*;
use crate::setup::setup_database;

/// The settings store over a fresh database file in `dir`, and its pool for
/// writing rows by hand.
async fn store(dir: &tempfile::TempDir) -> (SqliteSettingsRepository, SqlitePool) {
    let pool = setup_database(&dir.path().join("gglib.db"))
        .await
        .expect("the database opens");
    (SqliteSettingsRepository::new(pool.clone()), pool)
}

/// Store `value` as `key`'s row exactly as given, readable or not.
async fn put(pool: &SqlitePool, key: &str, value: &str) {
    sqlx::query("INSERT OR REPLACE INTO settings_kv (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(key)
        .bind(value)
        .bind(now())
        .execute(pool)
        .await
        .expect("row written");
}

/// `key`'s row as stored, or `None` when there is none.
async fn raw(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings_kv WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .expect("row read")
}

/// The pairing, roster, remote and proxy-key rows of a machine that joined
/// another, serves remote access with MCP on and admits one device, as the
/// store writes them.
fn kept_rows() -> Vec<(&'static str, String)> {
    let settings = Settings {
        proxy_api_key: Some("proxy-key".to_owned()),
        remote_pairing: Some(RemotePairing {
            ticket: "pipeabc".to_owned(),
            api_key: "far-key".to_owned(),
            default_model: None,
            port: None,
            name: None,
        }),
        remote_enabled: Some(true),
        remote_serve: Some(RemoteServe {
            mcp_servers: vec!["tavily".to_owned()],
            ..RemoteServe::default()
        }),
        remote_devices: Some(vec![Device {
            id: "dev-0a1b2c3d".to_owned(),
            label: None,
            joined_at: 1,
            redeemed_at: None,
            last_seen: None,
            peer: None,
            endpoint: None,
        }]),
        ..Settings::default()
    };
    let rows = rows_of(&settings).expect("rows");
    KEPT_ON_RESET
        .iter()
        .map(|key| (*key, rows[*key].as_ref().expect("set").to_string()))
        .collect()
}

/// A preference row a newer build could write: a mode this build has no
/// variant for. It is what made `show` and `reset` both fail.
#[tokio::test]
async fn a_reset_drops_a_preference_this_build_cannot_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, pool) = store(&dir).await;
    for (key, value) in kept_rows() {
        put(&pool, key, &value).await;
    }
    put(&pool, "loop_guard_mode", r#""strict""#).await;
    put(&pool, "proxy_port", "9191").await;
    assert!(repo.load().await.is_err(), "the record starts unreadable");

    let reset = repo.reset_preferences().await.expect("the reset is stored");

    assert_eq!(reset.dropped, ["loop_guard_mode"]);
    assert!(reset.kept_unreadable.is_empty());
    let stored = repo.load().await.expect("the record reads after the reset");
    assert_eq!(stored.loop_guard_mode, None);
    assert_eq!(stored.proxy_port, Settings::with_defaults().proxy_port);
    for (key, value) in kept_rows() {
        assert_eq!(raw(&pool, key).await, Some(value), "{key}");
    }
}

/// A kept row this build cannot read is left byte for byte as it was and
/// named, whatever else is dropped beside it, for each of the five.
#[tokio::test]
async fn a_kept_row_this_build_cannot_read_survives_a_reset() {
    for unreadable in KEPT_ON_RESET {
        let dir = tempfile::tempdir().expect("tempdir");
        let (repo, pool) = store(&dir).await;
        for (key, value) in kept_rows() {
            put(&pool, key, &value).await;
        }
        put(&pool, unreadable, "{not json").await;
        put(&pool, "loop_guard_mode", r#""strict""#).await;

        let reset = repo.reset_preferences().await.expect("the reset is stored");

        assert_eq!(reset.kept_unreadable, [unreadable]);
        assert_eq!(reset.dropped, ["loop_guard_mode"]);
        assert_eq!(raw(&pool, "loop_guard_mode").await, None);
        for (key, value) in kept_rows() {
            let want = if key == unreadable {
                "{not json"
            } else {
                &value
            };
            assert_eq!(raw(&pool, key).await.as_deref(), Some(want), "{key}");
        }
    }
}

/// A row no field of this build has is left alone when it is JSON, as a
/// whole record's read leaves it, and dropped when it is not.
#[tokio::test]
async fn a_row_no_field_has_is_dropped_only_when_it_is_not_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, pool) = store(&dir).await;
    put(&pool, "a_newer_builds_field", "7").await;
    put(&pool, "a_hand_edited_field", "seven").await;

    let reset = repo.reset_preferences().await.expect("the reset is stored");

    assert_eq!(reset.dropped, ["a_hand_edited_field"]);
    assert_eq!(
        raw(&pool, "a_newer_builds_field").await.as_deref(),
        Some("7")
    );
    assert_eq!(raw(&pool, "a_hand_edited_field").await, None);
}
