//! `SQLite` implementation of the `ChatHistoryRepository` trait.

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use gglib_core::{
    domain::branching::LineChat,
    domain::chat::{
        Conversation, ConversationUpdate, Message, MessageRole, NewConversation, NewMessage,
    },
    ports::chat_history::{ChatHistoryError, ChatHistoryRepository},
};

use super::{branch_rows, message_rows};

/// The columns [`conversation`] reads.
const CONVERSATION_COLUMNS: &str = "id, title, model_id, system_prompt, settings, \
     created_at, updated_at, branch_of, lineage_id";

/// A conversation's row as the domain type. Settings that do not parse read
/// as none.
fn conversation(row: &SqliteRow) -> Conversation {
    let settings_str: Option<String> = row.get("settings");
    Conversation {
        id: row.get("id"),
        title: row.get("title"),
        model_id: row.get("model_id"),
        system_prompt: row.get("system_prompt"),
        settings: settings_str.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        branch_of: row.get("branch_of"),
        lineage_id: row.get("lineage_id"),
    }
}

/// `SQLite` implementation of the `ChatHistoryRepository` trait.
///
/// This struct holds a connection pool and implements all CRUD operations
/// for chat conversations and messages using `SQLite`.
pub struct SqliteChatHistoryRepository {
    pool: SqlitePool,
}

impl SqliteChatHistoryRepository {
    /// Create a new `SQLite` chat history repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChatHistoryRepository for SqliteChatHistoryRepository {
    async fn create_conversation(&self, conv: NewConversation) -> Result<i64, ChatHistoryError> {
        let settings_str = conv
            .settings
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok());

        let result = sqlx::query(
            "INSERT INTO chat_conversations (title, model_id, system_prompt, settings) VALUES (?, ?, ?, ?)",
        )
        .bind(&conv.title)
        .bind(conv.model_id)
        .bind(conv.system_prompt)
        .bind(&settings_str)
        .execute(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(result.last_insert_rowid())
    }

    async fn list_conversations(&self) -> Result<Vec<Conversation>, ChatHistoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {CONVERSATION_COLUMNS} FROM chat_conversations ORDER BY updated_at DESC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;
        Ok(rows.iter().map(conversation).collect())
    }

    async fn get_conversation(&self, id: i64) -> Result<Option<Conversation>, ChatHistoryError> {
        let row = sqlx::query(&format!(
            "SELECT {CONVERSATION_COLUMNS} FROM chat_conversations WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;
        Ok(row.as_ref().map(conversation))
    }

    async fn update_conversation(
        &self,
        id: i64,
        update: ConversationUpdate,
    ) -> Result<(), ChatHistoryError> {
        if update.title.is_none()
            && update.system_prompt.is_none()
            && update.settings.is_none()
            && update.model_id.is_none()
        {
            return Ok(());
        }

        let row = sqlx::query(
            "SELECT title, model_id, system_prompt, settings FROM chat_conversations WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?
        .ok_or(ChatHistoryError::ConversationNotFound(id))?;

        let current_title: String = row.get("title");
        let current_model: Option<i64> = row.get("model_id");
        let current_prompt: Option<String> = row.get("system_prompt");
        let current_settings: Option<String> = row.get("settings");

        let next_title = update.title.unwrap_or(current_title);
        let next_model = update.model_id.unwrap_or(current_model);
        let next_prompt = update.system_prompt.unwrap_or(current_prompt);
        let next_settings = match update.settings {
            Some(Some(s)) => serde_json::to_string(&s).ok(),
            Some(None) => None,
            None => current_settings,
        };

        sqlx::query(
            "UPDATE chat_conversations SET title = ?, model_id = ?, system_prompt = ?, settings = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(next_title)
        .bind(next_model)
        .bind(next_prompt)
        .bind(next_settings)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete_conversation(&self, id: i64) -> Result<(), ChatHistoryError> {
        sqlx::query("DELETE FROM chat_conversations WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn get_conversation_count(&self) -> Result<i64, ChatHistoryError> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM chat_conversations")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(row.get("count"))
    }

    async fn get_messages(&self, conversation_id: i64) -> Result<Vec<Message>, ChatHistoryError> {
        let rows = sqlx::query(
            "SELECT id, conversation_id, role, content, metadata, created_at, origin_id 
             FROM chat_messages 
             WHERE conversation_id = ? 
             ORDER BY id ASC",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        let mut images = message_rows::images_by_message(&self.pool, conversation_id).await?;
        let messages = rows
            .iter()
            .map(|row| {
                let id: i64 = row.get("id");
                let role_str: String = row.get("role");
                let role = MessageRole::parse(&role_str).unwrap_or(MessageRole::User);
                let metadata_str: Option<String> = row.get("metadata");
                let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());
                Message {
                    id,
                    conversation_id: row.get("conversation_id"),
                    role,
                    content: row.get("content"),
                    created_at: row.get("created_at"),
                    metadata,
                    images: images.remove(&id).unwrap_or_default(),
                    origin_id: row.get("origin_id"),
                }
            })
            .collect();

        Ok(messages)
    }

    async fn save_message(&self, msg: NewMessage) -> Result<i64, ChatHistoryError> {
        let db = |e: sqlx::Error| ChatHistoryError::Database(e.to_string());
        // Dropped before `commit`, the transaction rolls back.
        let mut tx = self.pool.begin().await.map_err(db)?;
        let message_id = message_rows::insert(&mut tx, &msg).await?;
        message_rows::touch(&mut tx, msg.conversation_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(message_id)
    }

    async fn save_messages(&self, msgs: Vec<NewMessage>) -> Result<(), ChatHistoryError> {
        let db = |e: sqlx::Error| ChatHistoryError::Database(e.to_string());
        // Dropped before `commit`, the transaction rolls back.
        let mut tx = self.pool.begin().await.map_err(db)?;
        let mut touched = std::collections::BTreeSet::new();
        for msg in &msgs {
            message_rows::insert(&mut tx, msg).await?;
            touched.insert(msg.conversation_id);
        }
        for conversation_id in touched {
            message_rows::touch(&mut tx, conversation_id).await?;
        }
        tx.commit().await.map_err(db)
    }

    async fn replace_from(&self, from: i64, msg: NewMessage) -> Result<i64, ChatHistoryError> {
        let db = |e: sqlx::Error| ChatHistoryError::Database(e.to_string());
        // Dropped before `commit`, the transaction rolls back.
        let mut tx = self.pool.begin().await.map_err(db)?;
        // A write first, so the transaction holds the write lock from its
        // first statement: a read first would have to upgrade, and a write
        // that landed in between makes that upgrade fail. No row deleted
        // means `from` is not this conversation's; nothing has changed.
        let deleted = sqlx::query(
            "DELETE FROM chat_messages WHERE conversation_id = ?1 AND id >= ?2 \
             AND EXISTS (SELECT 1 FROM chat_messages WHERE id = ?2 AND conversation_id = ?1)",
        )
        .bind(msg.conversation_id)
        .bind(from)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if deleted.rows_affected() == 0 {
            return Err(ChatHistoryError::MessageNotFound(from));
        }
        let id = message_rows::insert(&mut tx, &msg).await?;
        message_rows::touch(&mut tx, msg.conversation_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(id)
    }

    async fn conversation_of_message(&self, id: i64) -> Result<Option<i64>, ChatHistoryError> {
        sqlx::query_scalar("SELECT conversation_id FROM chat_messages WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))
    }

    async fn delete_message_and_subsequent(&self, id: i64) -> Result<i64, ChatHistoryError> {
        let db = |e: sqlx::Error| ChatHistoryError::Database(e.to_string());
        // Dropped before `commit`, the transaction rolls back.
        let mut tx = self.pool.begin().await.map_err(db)?;
        // A write first, as in `replace_from`: the transaction holds the
        // write lock from its first statement. No row deleted means no
        // message has that id; nothing has changed.
        let deleted: Vec<i64> = sqlx::query_scalar(
            "DELETE FROM chat_messages WHERE id >= ?1 \
             AND conversation_id = (SELECT conversation_id FROM chat_messages WHERE id = ?1) \
             RETURNING conversation_id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let Some(&conversation_id) = deleted.first() else {
            return Err(ChatHistoryError::MessageNotFound(id));
        };
        message_rows::touch(&mut tx, conversation_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(i64::try_from(deleted.len()).unwrap_or(i64::MAX))
    }

    async fn fork(
        &self,
        source: i64,
        through: Option<i64>,
        then: Option<NewMessage>,
    ) -> Result<i64, ChatHistoryError> {
        branch_rows::fork(&self.pool, source, through, then).await
    }

    async fn lineage(&self, conversation_id: i64) -> Result<Vec<LineChat>, ChatHistoryError> {
        branch_rows::lineage(&self.pool, conversation_id).await
    }

    async fn get_message_count(&self, conversation_id: i64) -> Result<i64, ChatHistoryError> {
        let row =
            sqlx::query("SELECT COUNT(*) as count FROM chat_messages WHERE conversation_id = ?")
                .bind(conversation_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(row.get("count"))
    }
}

#[cfg(test)]
#[path = "sqlite_chat_history_repository_tests.rs"]
mod sqlite_chat_history_repository_tests;

#[cfg(test)]
#[path = "sqlite_chat_history_images_tests.rs"]
mod sqlite_chat_history_images_tests;

#[cfg(test)]
#[path = "sqlite_chat_branches_tests.rs"]
mod sqlite_chat_branches_tests;
