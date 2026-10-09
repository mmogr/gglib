//! The columns a branched chat is recorded by (ADR 0017).
//!
//! A `#[path]` child of `setup.rs`. `create_schema` adds them after the chat
//! tables. None has a foreign key: each names a row that may since have
//! been deleted, and a family outlives its first chat.

use anyhow::Result;
use sqlx::SqlitePool;

use super::add_column_if_missing;

/// Adds what a branch is recorded by, to a database that lacks it:
///
/// - `chat_messages.origin_id`: the message a copy copies, as first
///   written; NULL for a message written in its own chat.
/// - `chat_conversations.lineage_id`: the first chat of the family; NULL for
///   that chat itself.
/// - `chat_conversations.branch_of`: the chat a branch was made from.
///
/// A chat stored before them reads NULL in all three, which is what it is:
/// an original, and the first of a family of one. Nothing is backfilled.
pub(super) async fn add_branch_columns(pool: &SqlitePool) -> Result<()> {
    add_column_if_missing(pool, "chat_messages", "origin_id", "INTEGER").await?;
    add_column_if_missing(pool, "chat_conversations", "lineage_id", "INTEGER").await?;
    add_column_if_missing(pool, "chat_conversations", "branch_of", "INTEGER").await?;
    // A chat's family is read whenever it is opened.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_conversations_lineage \
         ON chat_conversations(lineage_id)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "setup_branches_tests.rs"]
mod setup_branches_tests;
