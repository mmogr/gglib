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
//! only a name exactly one server has, as the list's own read opened, and
//! never `builtin` (an SSE server it flags is deleted by step 6). The rows are read as raw JSON
//! and never as `Settings`, which would drop the list once that field is
//! gone, and fail on an unrelated row it cannot read.

use anyhow::Result;
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::repositories::sqlite_settings_repository::{stored_rows, values};

/// The settings row that says whether the tunnel is on.
const ENABLED: &str = "remote_enabled";

/// The settings row whose `mcpServers` was the list.
const SERVE: &str = "remote_serve";

/// The name gglib keeps for its own tools, never opened to a device.
const RESERVED: &str = "builtin";

/// Step 4: flag each listed name's one server. A list that cannot be read
/// opens nothing and never stops the boot. An SSE server flagged here is
/// deleted by step 6.
pub(super) async fn flag_listed_servers(conn: &mut SqliteConnection) -> Result<()> {
    let list = listed(stored_rows(conn).await?);
    if list.is_empty() {
        return Ok(());
    }
    let servers: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM mcp_servers")
        .fetch_all(&mut *conn)
        .await?;

    let mut flagged = 0_u64;
    for name in &list {
        let mut named = servers.iter().filter(|(_, has)| has == name);
        // Two servers sharing a name opened neither under the list.
        let (Some((id, _)), None) = (named.next(), named.next()) else {
            continue;
        };
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

#[cfg(test)]
#[path = "setup_remote_grant_tests.rs"]
mod setup_remote_grant_tests;
