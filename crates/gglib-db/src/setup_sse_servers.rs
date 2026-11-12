//! The SSE servers a library stored, deleted.
//!
//! A `#[path]` child of `setup.rs`. gglib never ran an SSE server: one could
//! be added, and every start, edit and listing refused or hid it. Now gglib
//! has no such type at all, and a row of it is refused when read, so the
//! rows go once, with the variables stored for them. The `type` column's
//! CHECK and the `url` column are left as they are: changing either means
//! rebuilding the table.

use anyhow::Result;
use sqlx::SqliteConnection;

/// Step 6: delete every `sse` server and its env rows.
///
/// The env rows go by their own statement, not by the foreign key's
/// cascade, which holds only on a connection with `foreign_keys` on.
pub(super) async fn delete_sse_servers(conn: &mut SqliteConnection) -> Result<()> {
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
    if deleted > 0 {
        tracing::info!(
            deleted,
            "deleted the SSE MCP servers, which gglib never ran"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_sse_servers_tests.rs"]
mod setup_sse_servers_tests;
