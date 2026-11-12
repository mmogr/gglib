//! The MCP server tables, and step 6, which brings an older library's to
//! their shape.
//!
//! A `#[path]` child of `setup.rs`. A server is a process gglib starts, so
//! `mcp_servers` keeps no type and no URL, and its lifecycle is `eager` or
//! `lazy`. A library from before had both columns, a CHECK that admitted
//! `manual`, and could hold SSE servers, which gglib never ran: step 6
//! deletes those with their variables, then rebuilds the two tables. It runs
//! after `retire_manual_lifecycle`, so no `manual` row is left to copy.

use anyhow::Result;
use sqlx::{SqliteConnection, SqlitePool};

/// The columns of `mcp_servers` a library made before step 6 has and a new
/// one does not.
const RETIRED_COLUMNS: [&str; 2] = ["type", "url"];

/// The index on the variables' server.
const ENV_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_mcp_env_server ON mcp_server_env(server_id)";

/// `mcp_servers`, as `table`.
fn servers_ddl(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {table} (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            allow_remote INTEGER NOT NULL DEFAULT 0,
            lifecycle TEXT NOT NULL DEFAULT 'lazy' CHECK (lifecycle IN ('eager', 'lazy')),
            command TEXT,
            resolved_path_cache TEXT,
            args TEXT NOT NULL DEFAULT '[]',
            cwd TEXT,
            path_extra TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_connected_at TEXT,
            is_valid INTEGER NOT NULL DEFAULT 0,
            last_error TEXT
        )"
    )
}

/// `mcp_server_env`, as `table`, its servers in `servers`.
fn env_ddl(table: &str, servers: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {table} (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            FOREIGN KEY (server_id) REFERENCES {servers}(id) ON DELETE CASCADE,
            UNIQUE(server_id, key)
        )"
    )
}

/// Make the two tables, where a library has none.
pub(super) async fn create_mcp_tables(pool: &SqlitePool) -> Result<()> {
    sqlx::query(&servers_ddl("mcp_servers"))
        .execute(pool)
        .await?;
    // Whether paired devices may use a server, closed unless set: a column a
    // library from before it lacks. Step 4 opens the servers the old list
    // named.
    super::add_column_if_missing(
        pool,
        "mcp_servers",
        "allow_remote",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    sqlx::query(&env_ddl("mcp_server_env", "mcp_servers"))
        .execute(pool)
        .await?;
    sqlx::query(ENV_INDEX).execute(pool).await?;
    Ok(())
}

/// Step 6: delete every SSE server and its variables, then rebuild both
/// tables without `type` and `url`. A library that has neither column, as
/// a new one, is left as it is.
///
/// `SQLite`'s table rebuild, inside the step's transaction, with foreign
/// keys on, which a transaction cannot turn off. So the variables move to a
/// new table first, pointing at the new servers table: dropping the old
/// servers table then cascades into nothing, and renaming the new one over
/// it rewrites the variables' reference to the name `mcp_servers`. Ids are
/// kept, and so is each table's `AUTOINCREMENT` high mark, so a deleted
/// server's id is never given to another.
pub(super) async fn rebuild_without_type(conn: &mut SqliteConnection) -> Result<()> {
    let retired: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('mcp_servers') WHERE name IN ('type', 'url')",
    )
    .fetch_one(&mut *conn)
    .await?;
    if retired == 0 {
        return Ok(());
    }

    sqlx::query(
        "DELETE FROM mcp_server_env \
         WHERE server_id IN (SELECT id FROM mcp_servers WHERE type = 'sse')",
    )
    .execute(&mut *conn)
    .await?;
    let deleted = sqlx::query("DELETE FROM mcp_servers WHERE type = 'sse'")
        .execute(&mut *conn)
        .await?
        .rows_affected();

    let servers_mark = high_mark(conn, "mcp_servers").await?;
    let env_mark = high_mark(conn, "mcp_server_env").await?;
    sqlx::query(&servers_ddl("mcp_servers_new"))
        .execute(&mut *conn)
        .await?;
    sqlx::query(&env_ddl("mcp_server_env_new", "mcp_servers_new"))
        .execute(&mut *conn)
        .await?;
    let columns = kept_columns(conn).await?;
    sqlx::query(&format!(
        "INSERT INTO mcp_servers_new ({columns}) SELECT {columns} FROM mcp_servers"
    ))
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO mcp_server_env_new (id, server_id, key, value) \
         SELECT id, server_id, key, value FROM mcp_server_env",
    )
    .execute(&mut *conn)
    .await?;

    for statement in [
        "DROP TABLE mcp_server_env",
        "DROP TABLE mcp_servers",
        "ALTER TABLE mcp_servers_new RENAME TO mcp_servers",
        "ALTER TABLE mcp_server_env_new RENAME TO mcp_server_env",
        ENV_INDEX,
    ] {
        sqlx::query(statement).execute(&mut *conn).await?;
    }
    restore_high_mark(conn, "mcp_servers", servers_mark).await?;
    restore_high_mark(conn, "mcp_server_env", env_mark).await?;

    tracing::info!(
        deleted,
        "deleted the SSE MCP servers, which gglib never ran, and the columns they used"
    );
    Ok(())
}

/// Every column of the old `mcp_servers` but the retired ones, quoted, in
/// its order.
async fn kept_columns(conn: &mut SqliteConnection) -> Result<String> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('mcp_servers') ORDER BY cid")
            .fetch_all(&mut *conn)
            .await?;
    Ok(columns
        .iter()
        .filter(|column| !RETIRED_COLUMNS.contains(&column.as_str()))
        .map(|column| format!("\"{}\"", column.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", "))
}

/// The highest id `table` has given, from `sqlite_sequence`.
async fn high_mark(conn: &mut SqliteConnection, table: &str) -> Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name = ?")
            .bind(table)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// Put `mark` back as `table`'s high mark, where the copy left it lower.
async fn restore_high_mark(
    conn: &mut SqliteConnection,
    table: &str,
    mark: Option<i64>,
) -> Result<()> {
    let Some(mark) = mark else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO sqlite_sequence (name, seq) SELECT ?1, ?2 \
         WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = ?1)",
    )
    .bind(table)
    .bind(mark)
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE sqlite_sequence SET seq = MAX(seq, ?2) WHERE name = ?1")
        .bind(table)
        .bind(mark)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The two tables as a library made before step 6 has them, for a test to
/// stand a library of that age up on this build's schema.
#[cfg(test)]
pub(super) async fn as_before_step_6(pool: &SqlitePool) {
    sqlx::raw_sql(
        "DROP TABLE mcp_server_env;
         DROP TABLE mcp_servers;
         CREATE TABLE mcp_servers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            type TEXT NOT NULL CHECK (type IN ('stdio', 'sse')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allow_remote INTEGER NOT NULL DEFAULT 0,
            lifecycle TEXT NOT NULL DEFAULT 'lazy' CHECK (lifecycle IN ('eager', 'lazy', 'manual')),
            command TEXT,
            resolved_path_cache TEXT,
            args TEXT NOT NULL DEFAULT '[]',
            cwd TEXT,
            path_extra TEXT,
            url TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_connected_at TEXT,
            is_valid INTEGER NOT NULL DEFAULT 0,
            last_error TEXT
         );
         CREATE TABLE mcp_server_env (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            FOREIGN KEY (server_id) REFERENCES mcp_servers(id) ON DELETE CASCADE,
            UNIQUE(server_id, key)
         );
         CREATE INDEX idx_mcp_env_server ON mcp_server_env(server_id);",
    )
    .execute(pool)
    .await
    .expect("the tables as before step 6");
}

#[cfg(test)]
#[path = "setup_mcp_servers_tests.rs"]
mod setup_mcp_servers_tests;
