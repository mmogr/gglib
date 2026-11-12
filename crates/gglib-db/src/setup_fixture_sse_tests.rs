//! Step 6 on a library 0.33.0 made: an SSE server added to a copy of the
//! fixture, as 0.33.0's `mcp add --type sse` stored one, is gone once this
//! build opens it, with its variable; the stdio servers stay whole, the file
//! is backed up first, and a second open changes nothing.

use std::path::Path;

use gglib_core::ports::{McpServerRepository, NoopGgufParser};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;

use super::super::backup_beside;
use super::{FIXTURE, FIXTURE_GENERATION, contents};
use crate::{SqliteMcpRepository, setup_database};

/// Add a server `name` of `kind`, with one variable, to the file at `db`
/// without opening it as gglib does.
async fn plant(db: &Path, name: &str, kind: &str) {
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(db))
        .await
        .unwrap();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO mcp_servers (name, type, enabled, lifecycle, command, args, url) \
         VALUES (?, ?, 1, 'lazy', ?, '[]', ?) RETURNING id",
    )
    .bind(name)
    .bind(kind)
    .bind((kind == "stdio").then_some("/bin/cat"))
    .bind((kind == "sse").then_some("http://localhost:3001/sse"))
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO mcp_server_env (server_id, key, value) VALUES (?, 'SSE_KEY', 'dg==')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

/// The names of the servers left, and of the servers whose variables are.
async fn left(pool: &SqlitePool) -> (Vec<String>, Vec<String>) {
    let servers = sqlx::query_scalar("SELECT name FROM mcp_servers ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap();
    let env = sqlx::query_scalar(
        "SELECT coalesce(s.name, '<none>') FROM mcp_server_env e \
         LEFT JOIN mcp_servers s ON s.id = e.server_id ORDER BY e.id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    (servers, env)
}

#[tokio::test]
async fn an_sse_server_is_deleted_and_the_stdio_ones_stay_whole() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("gglib.db");
    std::fs::copy(FIXTURE, &db).expect("copy the fixture");
    plant(&db, "fixture-remote", "sse").await;
    plant(&db, "fixture-cat", "stdio").await;

    let pool = setup_database(&db, &NoopGgufParser).await.unwrap();

    let names = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        left(&pool).await,
        (
            names(&["fixture-tools", "fixture-off", "fixture-cat"]),
            names(&["fixture-tools", "fixture-cat"]),
        )
    );
    let servers = SqliteMcpRepository::new(pool.clone()).list().await.unwrap();
    let cat = servers.iter().find(|s| s.name == "fixture-cat").unwrap();
    assert_eq!(cat.config.command.as_deref(), Some("/bin/cat"));
    assert_eq!(cat.env.len(), 1);
    let (generation, backup) = backup_beside(&db).expect("a backup");
    assert_eq!(generation, FIXTURE_GENERATION);
    let kept = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&backup)
            .read_only(true),
    )
    .await
    .unwrap();
    let sse: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mcp_servers WHERE type = 'sse'")
        .fetch_one(&kept)
        .await
        .unwrap();
    assert_eq!(
        sse, 1,
        "the backup is from before the step deleted anything"
    );
    kept.close().await;

    let first = contents(&pool).await;
    pool.close().await;
    let pool = setup_database(&db, &NoopGgufParser).await.unwrap();
    assert_eq!(
        contents(&pool).await,
        first,
        "a second open changes nothing"
    );
    pool.close().await;
}
