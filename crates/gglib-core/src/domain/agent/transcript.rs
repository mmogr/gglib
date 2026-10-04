//! Agent messages as the rows of a saved conversation.
//!
//! One mapping, shared by every writer of an agent transcript, so the CLI's
//! sessions and the daemon's runs are saved in the one shape the chat page
//! loads.

use crate::domain::chat::{MessageRole, NewMessage};

use super::messages::AgentMessage;

/// Map an [`AgentMessage`] to a [`NewMessage`] for database storage.
///
/// The mapping is 1:1 — each agent message becomes one DB row:
/// - `System` / `User` → role + content, no metadata; a user message's
///   images go with it, by id
/// - `Assistant` → text in `content`, tool calls (if any) in `metadata.tool_calls`
/// - `Tool` → result in `content`, `tool_call_id` in `metadata`
pub fn to_new_message(msg: &AgentMessage, conversation_id: i64) -> NewMessage {
    match msg {
        AgentMessage::System { content } => NewMessage {
            conversation_id,
            role: MessageRole::System,
            content: content.clone(),
            metadata: None,
            images: Vec::new(),
        },
        AgentMessage::User { content, images } => NewMessage {
            conversation_id,
            role: MessageRole::User,
            content: content.clone(),
            metadata: None,
            images: images.clone(),
        },
        AgentMessage::Assistant { content } => {
            let metadata = if content.tool_calls.is_empty() {
                None
            } else {
                serde_json::to_value(&content.tool_calls)
                    .ok()
                    .map(|tc| serde_json::json!({ "tool_calls": tc }))
            };
            NewMessage {
                conversation_id,
                role: MessageRole::Assistant,
                content: content.text.clone().unwrap_or_default(),
                metadata,
                images: Vec::new(),
            }
        }
        AgentMessage::Tool {
            tool_call_id,
            content,
        } => NewMessage {
            conversation_id,
            role: MessageRole::Tool,
            content: content.clone(),
            metadata: Some(serde_json::json!({ "tool_call_id": tool_call_id })),
            images: Vec::new(),
        },
    }
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod transcript_tests;
