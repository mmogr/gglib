//! Step 6 on a library as the build before it left one: the SSE rows go
//! with their variables, the tables lose `type` and `url`, every kept row
//! keeps its id and its variables, and no id is given out twice.

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

use super::super::create_schema;
use super::super::migrations::{LATEST, run, user_version};
use super::as_before_step_6;

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

/// A library at step 5, its MCP tables as that build made them.
async fn before_step_6() -> SqlitePool {
    let pool = library().await;
    as_before_step_6(&pool).await;
    sqlx::query("PRAGMA user_version = 5")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// Add a server `name` of `kind` with one variable, as an older build
/// stored it; its id.
async fn plant(pool: &SqlitePool, name: &str, kind: &str) -> i64 {
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
    id
}

/// Every server as `(id, name)`, and every variable as its server's id.
async fn left(pool: &SqlitePool) -> (Vec<(i64, String)>, Vec<i64>) {
    let servers = sqlx::query_as("SELECT id, name FROM mcp_servers ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap();
    let env = sqlx::query_scalar("SELECT server_id FROM mcp_server_env ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap();
    (servers, env)
}

async fn columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// The SSE server and its variable go; the stdio ones keep their ids and
/// variables; the columns go; the foreign key still holds, on the new table
/// by its old name; and the lifecycle CHECK admits `eager` and `lazy` only.
#[tokio::test]
async fn sse_rows_and_the_retired_columns_go_and_the_rest_keeps_its_ids() {
    let pool = before_step_6().await;
    let files = plant(&pool, "files", "stdio").await;
    plant(&pool, "remote", "sse").await;
    let search = plant(&pool, "search", "stdio").await;

    run(&pool, LATEST).await.unwrap();

    assert_eq!(
        left(&pool).await,
        (
            vec![(files, "files".to_owned()), (search, "search".to_owned())],
            vec![files, search],
        )
    );
    let servers = columns(&pool, "mcp_servers").await;
    assert!(
        !servers.iter().any(|c| c == "type" || c == "url"),
        "{servers:?}"
    );
    assert_eq!(user_version(&pool).await.unwrap(), LATEST);

    let fresh = library().await;
    assert_eq!(servers, columns(&fresh, "mcp_servers").await);
    let reference: String =
        sqlx::query_scalar("SELECT \"table\" FROM pragma_foreign_key_list('mcp_server_env')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reference, "mcp_servers");
    sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
        .bind(files)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(left(&pool).await.1, [search], "the cascade still holds");

    // The lifecycle is `eager` or `lazy`: the rebuilt CHECK refuses `manual`.
    for (lifecycle, admitted) in [("eager", true), ("lazy", true), ("manual", false)] {
        let written = sqlx::query("INSERT INTO mcp_servers (name, lifecycle) VALUES ('x', ?)")
            .bind(lifecycle)
            .execute(&pool)
            .await;
        assert_eq!(written.is_ok(), admitted, "{lifecycle}: {written:?}");
    }
}

/// The highest id is not given again: the next server is numbered after
/// the deleted SSE server, which had it.
#[tokio::test]
async fn a_deleted_servers_id_is_not_given_again() {
    let pool = before_step_6().await;
    plant(&pool, "files", "stdio").await;
    let remote = plant(&pool, "remote", "sse").await;

    run(&pool, LATEST).await.unwrap();

    let next: i64 =
        sqlx::query_scalar("INSERT INTO mcp_servers (name) VALUES ('next') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(next > remote, "{next} reuses an id up to {remote}");
}

/// A new library, already in the shape, is left as it is, and the step
/// runs once.
#[tokio::test]
async fn a_new_library_is_left_as_it_is() {
    let pool = library().await;
    sqlx::query("INSERT INTO mcp_servers (name) VALUES ('files')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA user_version = 5")
        .execute(&pool)
        .await
        .unwrap();

    run(&pool, LATEST).await.unwrap();

    assert_eq!(left(&pool).await.0, [(1, "files".to_owned())]);
    assert_eq!(user_version(&pool).await.unwrap(), LATEST);
}

/// A `manual` server, enabled or not, comes out a disabled `lazy` one, so a
/// server that never ran unasked still does not; an eager or lazy one keeps
/// its lifecycle and its switch.
#[tokio::test]
async fn a_manual_server_comes_out_lazy_and_disabled_and_the_rest_as_they_were() {
    let pool = before_step_6().await;
    let planted = [
        ("eager-off", "eager", false),
        ("eager-on", "eager", true),
        ("lazy-off", "lazy", false),
        ("lazy-on", "lazy", true),
        ("manual-off", "manual", false),
        ("manual-on", "manual", true),
    ];
    for (name, lifecycle, enabled) in planted {
        sqlx::query(
            "INSERT INTO mcp_servers (name, type, lifecycle, enabled) VALUES (?, 'stdio', ?, ?)",
        )
        .bind(name)
        .bind(lifecycle)
        .bind(enabled)
        .execute(&pool)
        .await
        .unwrap();
    }

    run(&pool, LATEST).await.unwrap();

    let servers: Vec<(String, String, bool)> =
        sqlx::query_as("SELECT name, lifecycle, enabled FROM mcp_servers ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap();
    let row = |name: &str, lifecycle: &str, enabled: bool| {
        (name.to_owned(), lifecycle.to_owned(), enabled)
    };
    assert_eq!(
        servers,
        [
            row("eager-off", "eager", false),
            row("eager-on", "eager", true),
            row("lazy-off", "lazy", false),
            row("lazy-on", "lazy", true),
            row("manual-off", "lazy", false),
            row("manual-on", "lazy", false),
        ]
    );
}
