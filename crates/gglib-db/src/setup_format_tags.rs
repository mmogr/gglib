//! Step 8: the `format:*` tags go.
//!
//! A `#[path]` child of `setup.rs`. Detection used to tag a tool-calling
//! model with the format its template writes calls in (`format:hermes`,
//! `format:qwen-xml` and five more), for the proxy's dialect parser to read.
//! The parser is gone, so nothing reads the tags; each one goes from every
//! model's `tags`, by its prefix (as written, so `Format:` is a user's), and
//! the rest keep their order.
//!
//! `tags` is read leniently, so a row whose text is not a JSON array is left
//! as it is rather than failing the open; the `CASE` makes sure `json_each`
//! never reads one. A row with no `format:*` tag is not rewritten.

use anyhow::Result;
use sqlx::SqliteConnection;

/// Every model's `tags`, its `format:*` tags taken out.
const STRIP: &str = "UPDATE models SET tags = (
        SELECT json_group_array(value) FROM (
            SELECT value FROM json_each(models.tags)
            WHERE value IS NULL OR value NOT GLOB 'format:*' ORDER BY key
        )
    )
    WHERE CASE WHEN json_valid(tags) AND json_type(tags) = 'array'
        THEN EXISTS (SELECT 1 FROM json_each(models.tags) WHERE value GLOB 'format:*')
        ELSE 0 END";

/// Step 8: strip the `format:*` tags. See the module docs.
pub(super) async fn strip_format_tags(conn: &mut SqliteConnection) -> Result<()> {
    let stripped = sqlx::query(STRIP)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if stripped > 0 {
        tracing::info!(stripped, "format:* tags removed from models");
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_format_tags_tests.rs"]
mod setup_format_tags_tests;
