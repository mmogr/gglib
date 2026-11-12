//! The step that deletes stored SSE servers: which rows go, which stay, and
//! that it runs once.

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

use super::super::create_schema;
use super::super::migrations::{LATEST, run, user_version};

/// One connection, so every statement and pragma reads the one database.
async fn library() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    create_schema(&pool).await.unwrap();
    pool
}

/// Add a server `name` of `kind` with one variable, as an older build
/// stored it.
async fn plant(pool: &SqlitePool, name: &str, kind: &str) {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO mcp_servers (name, type, command, url) VALUES (?, ?, 'npx', 'http://h/sse') \
         RETURNING id",
    )
    .bind(name)
    .bind(kind)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO mcp_server_env (server_id, key, value) VALUES (?, 'KEY', 'dg==')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

/// Every server and variable left, by name.
async fn left(pool: &SqlitePool) -> (Vec<String>, Vec<String>) {
    let servers = sqlx::query_scalar("SELECT name FROM mcp_servers ORDER BY name")
        .fetch_all(pool)
        .await
        .unwrap();
    let env = sqlx::query_scalar(
        "SELECT s.name FROM mcp_server_env e LEFT JOIN mcp_servers s ON s.id = e.server_id \
         ORDER BY s.name",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    (servers, env)
}

/// An `sse` server goes with its variables, even on a connection that does
/// not cascade, and a stdio one is kept whole.
#[tokio::test]
async fn an_sse_server_and_its_variables_go_and_a_stdio_one_stays() {
    let pool = library().await;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    plant(&pool, "remote", "sse").await;
    plant(&pool, "files", "stdio").await;
    sqlx::query("PRAGMA user_version = 5")
        .execute(&pool)
        .await
        .unwrap();

    run(&pool, LATEST).await.unwrap();

    let files = vec!["files".to_owned()];
    assert_eq!(left(&pool).await, (files.clone(), files));
    assert_eq!(user_version(&pool).await.unwrap(), LATEST);
}

/// Once stamped, a later `sse` row (an older build's, written while this
/// one had the file open) is left to the read, which refuses it.
#[tokio::test]
async fn the_step_runs_once() {
    let pool = library().await;
    run(&pool, LATEST).await.unwrap();
    plant(&pool, "remote", "sse").await;

    run(&pool, LATEST).await.unwrap();

    let remote = vec!["remote".to_owned()];
    assert_eq!(left(&pool).await, (remote.clone(), remote));
}
