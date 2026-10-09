//! A chat's family (ADR 0017): a branch copied from a chat, and the family
//! read as its branch points read it.

use sqlx::{Row, SqliteConnection, SqlitePool};

use gglib_core::domain::branching::{LineChat, LineRow};
use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::ports::chat_history::ChatHistoryError;

use super::message_rows;

fn db(error: &sqlx::Error) -> ChatHistoryError {
    ChatHistoryError::Database(error.to_string())
}

/// Branch `source` as `ChatHistoryRepository::fork` says, in one
/// transaction: dropped before `commit`, it writes nothing.
pub(super) async fn fork(
    pool: &SqlitePool,
    source: i64,
    through: Option<i64>,
    then: Option<NewMessage>,
) -> Result<i64, ChatHistoryError> {
    let mut tx = pool.begin().await.map_err(|e| db(&e))?;
    // A write first, as in `replace_from`: the transaction holds the write
    // lock from its first statement.
    let made = sqlx::query(
        "INSERT INTO chat_conversations \
           (title, model_id, system_prompt, settings, lineage_id, branch_of) \
         SELECT title, model_id, system_prompt, settings, COALESCE(lineage_id, id), id \
         FROM chat_conversations WHERE id = ?",
    )
    .bind(source)
    .execute(&mut *tx)
    .await
    .map_err(|e| db(&e))?;
    if made.rows_affected() == 0 {
        return Err(ChatHistoryError::ConversationNotFound(source));
    }
    let branch = made.last_insert_rowid();
    if let Some(through) = through {
        copy_through(&mut tx, source, through, branch).await?;
    }
    if let Some(mut then) = then {
        then.conversation_id = branch;
        message_rows::insert(&mut tx, &then).await?;
    }
    tx.commit().await.map_err(|e| db(&e))?;
    Ok(branch)
}

/// Copy `source`'s messages but system ones, as far as `through`, to
/// `branch`, each with its images.
async fn copy_through(
    tx: &mut SqliteConnection,
    source: i64,
    through: i64,
    branch: i64,
) -> Result<(), ChatHistoryError> {
    let held = sqlx::query("SELECT 1 FROM chat_messages WHERE id = ? AND conversation_id = ?")
        .bind(through)
        .bind(source)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db(&e))?;
    if held.is_none() {
        return Err(ChatHistoryError::MessageNotFound(through));
    }
    let originals: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM chat_messages \
         WHERE conversation_id = ? AND id <= ? AND role != 'system' ORDER BY id",
    )
    .bind(source)
    .bind(through)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| db(&e))?;
    for original in originals {
        let copy = sqlx::query(
            "INSERT INTO chat_messages \
               (conversation_id, role, content, metadata, created_at, origin_id) \
             SELECT ?, role, content, metadata, created_at, COALESCE(origin_id, id) \
             FROM chat_messages WHERE id = ?",
        )
        .bind(branch)
        .bind(original)
        .execute(&mut *tx)
        .await
        .map_err(|e| db(&e))?
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO message_attachments (message_id, attachment_id, position) \
             SELECT ?, attachment_id, position FROM message_attachments WHERE message_id = ?",
        )
        .bind(copy)
        .bind(original)
        .execute(&mut *tx)
        .await
        .map_err(|e| db(&e))?;
    }
    Ok(())
}

/// Every chat of `conversation_id`'s family, as
/// `ChatHistoryRepository::lineage` says. A message's text is read only as
/// far as a preview needs it.
pub(super) async fn lineage(
    pool: &SqlitePool,
    conversation_id: i64,
) -> Result<Vec<LineChat>, ChatHistoryError> {
    let rows = sqlx::query(
        "SELECT c.id AS chat, c.updated_at, m.id, COALESCE(m.origin_id, m.id) AS key, m.role, \
                substr(ltrim(m.content, char(9, 10, 13, 32)), 1, 400) AS text, \
                (SELECT COUNT(*) FROM message_attachments a WHERE a.message_id = m.id) AS images \
         FROM (SELECT COALESCE(lineage_id, id) AS l FROM chat_conversations WHERE id = ?) f \
         JOIN chat_conversations c ON c.id = f.l OR c.lineage_id = f.l \
         LEFT JOIN chat_messages m ON m.conversation_id = c.id AND m.role != 'system' \
         ORDER BY c.id, m.id",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .map_err(|e| db(&e))?;
    let mut family: Vec<LineChat> = Vec::new();
    for row in &rows {
        let chat: i64 = row.get("chat");
        if family
            .last()
            .is_none_or(|last| last.conversation_id != chat)
        {
            family.push(LineChat {
                conversation_id: chat,
                updated_at: row.get("updated_at"),
                rows: Vec::new(),
            });
        }
        let Some(id) = row.get::<Option<i64>, _>("id") else {
            continue;
        };
        let role: String = row.get("role");
        let images: i64 = row.get("images");
        if let Some(last) = family.last_mut() {
            last.rows.push(LineRow {
                id,
                key: row.get("key"),
                role: MessageRole::parse(&role).unwrap_or(MessageRole::User),
                text: row.get("text"),
                images: usize::try_from(images).unwrap_or_default(),
            });
        }
    }
    Ok(family)
}

#[cfg(test)]
#[path = "branch_rows_tests.rs"]
mod branch_rows_tests;
