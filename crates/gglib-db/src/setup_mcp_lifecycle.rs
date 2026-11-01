//! The `manual` MCP lifecycle, retired.
//!
//! A `#[path]` child of `setup.rs`. Until #1699 a server could be `manual`:
//! never started on its own, and its tools not offered until something
//! started it, so in a chat it acted as a disabled server. That lifecycle is
//! gone and the row reader no longer parses it, so this turns each stored
//! `manual` server into what it behaved as: `lazy` and disabled. Enabled
//! again, it starts on first use, like any lazy server.
//!
//! Run at every open, not on the `user_version` ladder: the `UPDATE` is its
//! own guard, so a second run finds nothing to change, and an older build,
//! whose schema still admits `manual`, can write one again after a newer
//! build stamped the file. An older build reads `lazy` fine, so the schema's
//! generation is unchanged.

use anyhow::Result;
use sqlx::SqlitePool;

/// Turn every `manual` server into a disabled `lazy` one.
pub(super) async fn retire_manual_lifecycle(pool: &SqlitePool) -> Result<()> {
    let retired = sqlx::query(
        "UPDATE mcp_servers SET lifecycle = 'lazy', enabled = 0 WHERE lifecycle = 'manual'",
    )
    .execute(pool)
    .await?
    .rows_affected();
    if retired > 0 {
        tracing::info!(
            retired,
            "made the MCP servers that were `manual` lazy and disabled; enable one to use it"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_mcp_lifecycle_tests.rs"]
mod setup_mcp_lifecycle_tests;
