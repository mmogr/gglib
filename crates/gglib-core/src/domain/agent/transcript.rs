//! Agent messages as the rows of a saved conversation, and the rows back as
//! the messages its next turn starts from.
//!
//! One mapping, shared by every writer of an agent transcript, so the CLI's
//! sessions and the daemon's runs are saved in the one shape the chat page
//! loads. And one reading back, shared by every surface that continues a
//! saved chat, so a row means the same to a resumed CLI session as to a
//! paired device's turn.

use crate::domain::chat::{Message, MessageRole, NewMessage};

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

/// A saved conversation as the messages its next turn starts from.
///
/// `prompt`, trimmed, is the system message, unless that leaves nothing;
/// then comes every saved row but a system one, in order. The prompt is the
/// conversation's own, or the one a surface puts in its place, and is never
/// read from a row: a saved system row would send it twice.
#[must_use]
pub fn saved_history(prompt: Option<&str>, rows: &[Message]) -> Vec<AgentMessage> {
    let prompt = prompt.map(str::trim).filter(|p| !p.is_empty());
    let system = prompt.map(|p| AgentMessage::System {
        content: p.to_owned(),
    });
    let saved = rows.iter().filter(|row| row.role != MessageRole::System);
    system
        .into_iter()
        .chain(saved.map(Message::to_agent_message))
        .collect()
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod transcript_tests;
