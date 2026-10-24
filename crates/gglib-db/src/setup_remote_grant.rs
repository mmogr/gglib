//! The MCP servers the remote tunnel's list opened, flagged on their own
//! rows.
//!
//! A `#[path]` child of `setup.rs`. Until ADR 0025 the servers a paired
//! device's runs were offered were a list of names, `gglib remote enable
//! --allow-mcp` stored in the `remote_serve` row as `mcpServers`. Now each
//! server's row says so itself (`mcp_servers.allow_remote`) and nothing keeps
//! a list, so this copies the list onto the rows, once, so that an upgrade
//! keeps open what was open.
//!
//! It never opens more. It copies only from a machine whose tunnel was on,
//! since one that was off reset its list at its next `enable` anyway; and
//! only a name that exactly one stdio server has, as the list's own read
//! (`the_one`) opened, and never `builtin`. The rows are read as raw JSON
//! and never as `Settings`, which would drop the list once that field is
//! gone, and fail on an unrelated row it cannot read.

use anyhow::Result;
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqliteExecutor, SqlitePool};

use crate::repositories::sqlite_settings_repository::{stored_rows, values};

/// `PRAGMA user_version` once the old list's servers are flagged.
///
/// The fourth version this project has assigned, above
/// [`UNSTORED_RECIPE_SCHEMA_VERSION`](super::recipes::UNSTORED_RECIPE_SCHEMA_VERSION),
/// which goes on meaning what it meant; anything later takes a higher number
/// still.
pub(super) const REMOTE_GRANT_SCHEMA_VERSION: i64 = 4;

/// The settings row that says whether the tunnel is on.
const ENABLED: &str = "remote_enabled";

/// The settings row whose `mcpServers` was the list.
const SERVE: &str = "remote_serve";

/// The name gglib keeps for its own tools, never opened to a device.
const RESERVED: &str = "builtin";

/// Migration: flag the servers the old list named, once per database.
///
/// Gated on `user_version` and run in one `BEGIN IMMEDIATE` transaction, as
/// the recipes step is, and asked again once the lock is held, so of two
/// processes the second finds the work done and a later stamp is not
/// lowered. Stamped whatever the rows held: a list that cannot be read opens
/// nothing and never stops the boot.
pub(super) async fn flag_listed_servers(pool: &SqlitePool) -> Result<()> {
    if user_version(pool).await? >= REMOTE_GRANT_SCHEMA_VERSION {
        return Ok(());
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if user_version(&mut *tx).await? >= REMOTE_GRANT_SCHEMA_VERSION {
        return Ok(());
    }
    flag(&mut tx).await?;
    // Not bindable as a parameter: SQLite requires a literal here.
    sqlx::query(&format!(
        "PRAGMA user_version = {REMOTE_GRANT_SCHEMA_VERSION}"
    ))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Flag each listed name's one stdio server.
async fn flag(conn: &mut SqliteConnection) -> Result<()> {
    let list = listed(stored_rows(conn).await?);
    if list.is_empty() {
        return Ok(());
    }
    let servers: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, name, type FROM mcp_servers")
            .fetch_all(&mut *conn)
            .await?;

    let mut flagged = 0_u64;
    for name in &list {
        let mut named = servers.iter().filter(|(_, has, _)| has == name);
        // Two servers sharing a name opened neither under the list.
        let (Some((id, _, kind)), None) = (named.next(), named.next()) else {
            continue;
        };
        if kind != "stdio" {
            continue;
        }
        flagged += sqlx::query("UPDATE mcp_servers SET allow_remote = 1 WHERE id = ?")
            .bind(id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    }

    tracing::info!(
        listed = list.len(),
        flagged,
        "opened to paired devices the MCP servers the remote tunnel's list named"
    );
    Ok(())
}

/// The names the list held, trimmed, without a blank one or `builtin`, when
/// the tunnel was on; none when it was off or either row cannot be read.
///
/// Only the two rows are read, so a row of another setting this build cannot
/// read does not close what the list opened.
fn listed(rows: Vec<SqliteRow>) -> Vec<String> {
    let ours: Vec<SqliteRow> = rows
        .into_iter()
        .filter(|row| {
            row.try_get::<String, _>("key")
                .is_ok_and(|key| key == ENABLED || key == SERVE)
        })
        .collect();
    let Ok(record) = values(&ours) else {
        // Not the reason: it can quote the value of the row it stopped at.
        tracing::warn!(
            "the remote tunnel's settings cannot be read, so no MCP server it \
             listed is opened to paired devices; `gglib mcp allow-remote` opens one"
        );
        return Vec::new();
    };
    if record.get(ENABLED) != Some(&Value::Bool(true)) {
        return Vec::new();
    }
    let Some(Value::Array(list)) = record.get(SERVE).and_then(|serve| serve.get("mcpServers"))
    else {
        return Vec::new();
    };
    let mut names: Vec<String> = list
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != RESERVED)
        .map(str::to_owned)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

async fn user_version(conn: impl SqliteExecutor<'_>) -> sqlx::Result<i64> {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(conn)
        .await
}

#[cfg(test)]
#[path = "setup_remote_grant_tests.rs"]
mod setup_remote_grant_tests;
