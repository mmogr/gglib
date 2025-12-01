//! Chat history service for managing conversations and messages in SQLite.
//!
//! This module provides functions for:
//! - Creating and managing chat conversations
//! - Storing and retrieving chat messages
//! - Linking conversations to models
//! - Auto-updating conversation timestamps

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateConversationRequest {
    pub title: String,
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMessageRequest {
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
}

/// Create a new chat conversation
pub async fn create_conversation(
    pool: &SqlitePool,
    title: String,
    model_id: Option<i64>,
    system_prompt: Option<String>,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO chat_conversations (title, model_id, system_prompt) VALUES (?, ?, ?)",
    )
    .bind(&title)
    .bind(model_id)
    .bind(system_prompt)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Get all conversations, ordered by most recently updated
pub async fn get_conversations(pool: &SqlitePool) -> Result<Vec<Conversation>> {
    let rows = sqlx::query(
        "SELECT id, title, model_id, system_prompt, created_at, updated_at 
         FROM chat_conversations 
         ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;

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

/// Get a specific conversation by ID
pub async fn get_conversation(
    pool: &SqlitePool,
    conversation_id: i64,
) -> Result<Option<Conversation>> {
    let row = sqlx::query(
        "SELECT id, title, model_id, system_prompt, created_at, updated_at 
         FROM chat_conversations 
         WHERE id = ?",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Conversation {
        id: r.get("id"),
        title: r.get("title"),
        model_id: r.get("model_id"),
        system_prompt: r.get("system_prompt"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

/// Get all messages for a conversation, ordered chronologically
pub async fn get_messages(pool: &SqlitePool, conversation_id: i64) -> Result<Vec<Message>> {
    let rows = sqlx::query(
        "SELECT id, conversation_id, role, content, created_at 
         FROM chat_messages 
         WHERE conversation_id = ? 
         ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;

    let messages = rows
        .iter()
        .map(|row| Message {
            id: row.get("id"),
            conversation_id: row.get("conversation_id"),
            role: row.get("role"),
            content: row.get("content"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(messages)
}

/// Save a new message and update conversation timestamp
pub async fn save_message(
    pool: &SqlitePool,
    conversation_id: i64,
    role: String,
    content: String,
) -> Result<i64> {
    // Validate role
    if !["system", "user", "assistant"].contains(&role.as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid role: must be system, user, or assistant"
        ));
    }

    // Insert message
    let result =
        sqlx::query("INSERT INTO chat_messages (conversation_id, role, content) VALUES (?, ?, ?)")
            .bind(conversation_id)
            .bind(&role)
            .bind(&content)
            .execute(pool)
            .await?;

    let message_id = result.last_insert_rowid();

    // Update conversation timestamp
    sqlx::query("UPDATE chat_conversations SET updated_at = datetime('now') WHERE id = ?")
        .bind(conversation_id)
        .execute(pool)
        .await?;

    Ok(message_id)
}

/// Update conversation metadata (title/system prompt)
pub async fn update_conversation(
    pool: &SqlitePool,
    conversation_id: i64,
    new_title: Option<String>,
    system_prompt: Option<Option<String>>,
) -> Result<()> {
    if new_title.is_none() && system_prompt.is_none() {
        return Ok(());
    }

    let row = sqlx::query("SELECT title, system_prompt FROM chat_conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_one(pool)
        .await?;

    let current_title: String = row.get("title");
    let current_prompt: Option<String> = row.get("system_prompt");

    let next_title = new_title.unwrap_or(current_title);
    let next_prompt = system_prompt.unwrap_or(current_prompt);

    sqlx::query(
        "UPDATE chat_conversations SET title = ?, system_prompt = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(next_title)
    .bind(next_prompt)
    .bind(conversation_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a conversation and all its messages (cascade)
pub async fn delete_conversation(pool: &SqlitePool, conversation_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM chat_conversations WHERE id = ?")
        .bind(conversation_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Get conversation count
pub async fn get_conversation_count(pool: &SqlitePool) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM chat_conversations")
        .fetch_one(pool)
        .await?;

    Ok(row.get("count"))
}

/// Get message count for a conversation
pub async fn get_message_count(pool: &SqlitePool, conversation_id: i64) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) as count FROM chat_messages WHERE conversation_id = ?")
        .bind(conversation_id)
        .fetch_one(pool)
        .await?;

    Ok(row.get("count"))
}

/// Update a message's content by ID
pub async fn update_message(pool: &SqlitePool, message_id: i64, content: String) -> Result<()> {
    let result = sqlx::query("UPDATE chat_messages SET content = ? WHERE id = ?")
        .bind(&content)
        .bind(message_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Message not found: {}", message_id));
    }

    // Update the conversation timestamp
    sqlx::query(
        "UPDATE chat_conversations SET updated_at = datetime('now') 
         WHERE id = (SELECT conversation_id FROM chat_messages WHERE id = ?)",
    )
    .bind(message_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a message and all subsequent messages in the same conversation.
/// Returns the number of messages deleted.
pub async fn delete_message_and_subsequent(pool: &SqlitePool, message_id: i64) -> Result<i64> {
    // First get the conversation_id for this message
    let row = sqlx::query("SELECT conversation_id FROM chat_messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(pool)
        .await?;

    let conversation_id: i64 = match row {
        Some(r) => r.get("conversation_id"),
        None => return Err(anyhow::anyhow!("Message not found: {}", message_id)),
    };

    // Delete the target message and all messages with higher IDs in the same conversation
    let result = sqlx::query(
        "DELETE FROM chat_messages WHERE conversation_id = ? AND id >= ?",
    )
    .bind(conversation_id)
    .bind(message_id)
    .execute(pool)
    .await?;

    // Update the conversation timestamp
    sqlx::query("UPDATE chat_conversations SET updated_at = datetime('now') WHERE id = ?")
        .bind(conversation_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() as i64)
}
