//! The `SQLite` settings repository reading rows an older build wrote: rows
//! for fields that did not exist yet, and rows for fields that are gone.

use super::*;

/// A database written before these fields existed has no rows for them.
/// `Settings` is `#[serde(default)]` with `Option` fields, so the absent
/// rows deserialize to `None` rather than failing the whole load — which
/// is why no migration is needed for the new columns.
#[tokio::test]
async fn test_load_tolerates_rows_written_before_new_fields_existed() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await.unwrap();

    // Simulate a pre-existing DB: only an old key is present.
    sqlx::query(
        "INSERT INTO settings_kv (key, value, updated_at) VALUES ('proxy_port', '9090', '')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let loaded = repo.load().await.expect("older row sets still load");
    assert_eq!(loaded.proxy_port, Some(9090));
    assert_eq!(loaded.proxy_host, None);
    assert_eq!(loaded.proxy_cache_on_disk, None);
}

/// The tool-turn temperature cap is gone, and so is its setting. A library
/// that stored it, under its name or the `tool_call_floor` name before that,
/// still loads every other setting: the old rows are ignored.
#[tokio::test]
async fn the_rows_of_the_deleted_temperature_cap_are_ignored() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await.unwrap();
    for (key, value) in [
        ("agentic_sampling", "false"),
        ("tool_call_floor", "true"),
        ("proxy_port", "9090"),
        ("trust_client_sampling", "true"),
    ] {
        sqlx::query("INSERT INTO settings_kv (key, value, updated_at) VALUES (?, ?, '')")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
    }

    let loaded = repo.load().await.expect("the old rows do not stop a load");
    assert_eq!(
        loaded,
        Settings {
            proxy_port: Some(9090),
            trust_client_sampling: Some(true),
            ..Settings::default()
        }
    );
}

/// Serve rows older builds wrote, with their `/mcp` switch on or their list
/// of MCP servers, still read: neither key is a field now, since a server is
/// opened to paired devices by a flag on its own row, so the whole settings
/// load does not fail on either.
#[tokio::test]
async fn a_serve_row_an_older_build_wrote_still_reads() {
    for older in [
        r#"{"allowMcp":true,"relay":null,"discovery":true}"#,
        r#"{"mcpServers":["tavily"],"relay":null,"discovery":true}"#,
    ] {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let repo = SqliteSettingsRepository::new(pool.clone());
        repo.ensure_table().await.unwrap();
        sqlx::query(
            "INSERT INTO settings_kv (key, value, updated_at) VALUES ('remote_serve', ?, '')",
        )
        .bind(older)
        .execute(&pool)
        .await
        .unwrap();

        let serve = repo.load().await.expect("an older serve row still loads");
        let serve = serve.remote_serve.expect("and is read as a serve row");
        assert!(serve.discovery && serve.relay.is_none(), "{older}");
    }
}
