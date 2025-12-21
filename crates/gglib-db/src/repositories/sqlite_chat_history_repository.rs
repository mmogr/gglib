//! `SQLite` implementation of the `ChatHistoryRepository` trait.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use gglib_core::{
    domain::chat::{
        Conversation, ConversationUpdate, Message, MessageRole, NewConversation, NewMessage,
    },
    ports::chat_history::{ChatHistoryError, ChatHistoryRepository},
};

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
        let result = sqlx::query(
            "INSERT INTO chat_conversations (title, model_id, system_prompt) VALUES (?, ?, ?)",
        )
        .bind(&conv.title)
        .bind(conv.model_id)
        .bind(conv.system_prompt)
        .execute(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(result.last_insert_rowid())
    }

    async fn list_conversations(&self) -> Result<Vec<Conversation>, ChatHistoryError> {
        let rows = sqlx::query(
            "SELECT id, title, model_id, system_prompt, created_at, updated_at 
             FROM chat_conversations 
             ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        let conversations = rows
            .iter()
            .map(|row| Conversation {
                id: row.get("id"),
                title: row.get("title"),
                model_id: row.get("model_id"),
                system_prompt: row.get("system_prompt"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect();

        Ok(conversations)
    }

    async fn get_conversation(&self, id: i64) -> Result<Option<Conversation>, ChatHistoryError> {
        let row = sqlx::query(
            "SELECT id, title, model_id, system_prompt, created_at, updated_at 
             FROM chat_conversations 
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(row.map(|r| Conversation {
            id: r.get("id"),
            title: r.get("title"),
            model_id: r.get("model_id"),
            system_prompt: r.get("system_prompt"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn update_conversation(
        &self,
        id: i64,
        update: ConversationUpdate,
    ) -> Result<(), ChatHistoryError> {
        if update.title.is_none() && update.system_prompt.is_none() {
            return Ok(());
        }

        let row = sqlx::query("SELECT title, system_prompt FROM chat_conversations WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?
            .ok_or(ChatHistoryError::ConversationNotFound(id))?;

        let current_title: String = row.get("title");
        let current_prompt: Option<String> = row.get("system_prompt");

        let next_title = update.title.unwrap_or(current_title);
        let next_prompt = update.system_prompt.unwrap_or(current_prompt);

        sqlx::query(
            "UPDATE chat_conversations SET title = ?, system_prompt = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(next_title)
        .bind(next_prompt)
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
            "SELECT id, conversation_id, role, content, created_at 
             FROM chat_messages 
             WHERE conversation_id = ? 
             ORDER BY created_at ASC",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        let messages = rows
            .iter()
            .map(|row| {
                let role_str: String = row.get("role");
                let role = MessageRole::parse(&role_str).unwrap_or(MessageRole::User);
                Message {
                    id: row.get("id"),
                    conversation_id: row.get("conversation_id"),
                    role,
                    content: row.get("content"),
                    created_at: row.get("created_at"),
                }
            })
            .collect();

        Ok(messages)
    }

    async fn save_message(&self, msg: NewMessage) -> Result<i64, ChatHistoryError> {
        // Insert message
        let result = sqlx::query(
            "INSERT INTO chat_messages (conversation_id, role, content) VALUES (?, ?, ?)",
        )
        .bind(msg.conversation_id)
        .bind(msg.role.as_str())
        .bind(&msg.content)
        .execute(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        let message_id = result.last_insert_rowid();

        // Update conversation timestamp
        sqlx::query("UPDATE chat_conversations SET updated_at = datetime('now') WHERE id = ?")
            .bind(msg.conversation_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(message_id)
    }

    async fn update_message(&self, id: i64, content: String) -> Result<(), ChatHistoryError> {
        let result = sqlx::query("UPDATE chat_messages SET content = ? WHERE id = ?")
            .bind(&content)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ChatHistoryError::MessageNotFound(id));
        }

        // Update the conversation timestamp
        sqlx::query(
            "UPDATE chat_conversations SET updated_at = datetime('now') 
             WHERE id = (SELECT conversation_id FROM chat_messages WHERE id = ?)",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete_message_and_subsequent(&self, id: i64) -> Result<i64, ChatHistoryError> {
        // First get the conversation_id for this message
        let row = sqlx::query("SELECT conversation_id FROM chat_messages WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?
            .ok_or(ChatHistoryError::MessageNotFound(id))?;

        let conversation_id: i64 = row.get("conversation_id");

        // Delete the target message and all messages with higher IDs in the same conversation
        let result = sqlx::query("DELETE FROM chat_messages WHERE conversation_id = ? AND id >= ?")
            .bind(conversation_id)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        // Update the conversation timestamp
        sqlx::query("UPDATE chat_conversations SET updated_at = datetime('now') WHERE id = ?")
            .bind(conversation_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ChatHistoryError::Database(e.to_string()))?;

        Ok(result.rows_affected() as i64)
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
