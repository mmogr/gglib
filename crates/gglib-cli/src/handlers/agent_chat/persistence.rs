//! Conversation persistence for CLI agent sessions.
//!
//! Saves agent messages to the `chat_conversations` / `chat_messages` tables
//! so they appear in the GUI conversation list and can later be resumed.

use anyhow::Result;
use chrono::Local;

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewConversation, NewMessage};
use gglib_core::services::ChatHistoryService;

/// Tracks a persisted conversation and the number of messages already saved,
/// so subsequent calls to [`Conversation::save_new`] only write the delta.
pub struct Conversation<'a> {
    service: &'a ChatHistoryService,
    pub id: i64,
    saved: usize,
}

impl<'a> Conversation<'a> {
    /// Create a new conversation with a timestamp-based title.
    pub async fn create(
        service: &'a ChatHistoryService,
        system_prompt: Option<String>,
        model_id: Option<i64>,
        settings: Option<ConversationSettings>,
    ) -> Result<Conversation<'a>> {
        let title = format!("Agent session {}", Local::now().format("%Y-%m-%d %H:%M"));
        let id = service
            .create_conversation_with_settings(NewConversation {
                title,
                model_id,
                system_prompt,
                settings,
            })
            .await?;
        Ok(Conversation {
            service,
            id,
            saved: 0,
        })
    }

    /// Resume an existing conversation for continued persistence.
    ///
    /// Loads the existing message count so [`save_new`] only persists the delta.
    pub async fn resume(
        service: &'a ChatHistoryService,
        id: i64,
        existing_message_count: usize,
    ) -> Conversation<'a> {
        Conversation {
            service,
            id,
            saved: existing_message_count,
        }
    }

    /// Persist any messages added since the last call.
    ///
    /// Errors are logged as warnings and swallowed — persistence must never
    /// break the interactive session.
    pub async fn save_new(&mut self, messages: &[AgentMessage]) {
        for msg in messages.iter().skip(self.saved) {
            let new_msg = to_new_message(msg, self.id);
            if let Err(e) = self.service.save_message(new_msg).await {
                tracing::warn!("failed to persist agent message: {e}");
            }
        }
        self.saved = messages.len();
    }
}

/// Map an [`AgentMessage`] to a [`NewMessage`] for database storage.
///
/// The mapping is 1:1 — each agent message becomes one DB row:
/// - `System` / `User` → role + content, no metadata
/// - `Assistant` → text in `content`, tool calls (if any) in `metadata.tool_calls`
/// - `Tool` → result in `content`, `tool_call_id` in `metadata`
fn to_new_message(msg: &AgentMessage, conversation_id: i64) -> NewMessage {
    match msg {
        AgentMessage::System { content } => NewMessage {
            conversation_id,
            role: MessageRole::System,
            content: content.clone(),
            metadata: None,
        },
        AgentMessage::User { content } => NewMessage {
            conversation_id,
            role: MessageRole::User,
            content: content.clone(),
            metadata: None,
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
        },
    }
}

#[cfg(test)]
mod tests {
    use gglib_core::domain::agent::{AssistantContent, tool_types::ToolCall};

    use super::*;

    #[test]
    fn system_message_maps_correctly() {
        let msg = AgentMessage::System {
            content: "You are helpful.".into(),
        };
        let out = to_new_message(&msg, 42);
        assert_eq!(out.role, MessageRole::System);
        assert_eq!(out.content, "You are helpful.");
        assert!(out.metadata.is_none());
    }

    #[test]
    fn user_message_maps_correctly() {
        let msg = AgentMessage::User {
            content: "Hello".into(),
        };
        let out = to_new_message(&msg, 42);
        assert_eq!(out.role, MessageRole::User);
        assert_eq!(out.content, "Hello");
        assert!(out.metadata.is_none());
    }

    #[test]
    fn assistant_text_only() {
        let msg = AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("The answer is 4.".into()),
                tool_calls: vec![],
            },
        };
        let out = to_new_message(&msg, 42);
        assert_eq!(out.role, MessageRole::Assistant);
        assert_eq!(out.content, "The answer is 4.");
        assert!(out.metadata.is_none());
    }

    #[test]
    fn assistant_with_tool_calls() {
        let msg = AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("Let me check.".into()),
                tool_calls: vec![ToolCall {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "src/main.rs"}),
                }],
            },
        };
        let out = to_new_message(&msg, 42);
        assert_eq!(out.role, MessageRole::Assistant);
        assert_eq!(out.content, "Let me check.");
        let meta = out.metadata.unwrap();
        let calls = meta["tool_calls"].as_array().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["id"], "call_1");
        assert_eq!(calls[0]["name"], "read_file");
    }

    #[test]
    fn assistant_tool_calls_no_text() {
        let msg = AgentMessage::Assistant {
            content: AssistantContent {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "call_2".into(),
                    name: "list_directory".into(),
                    arguments: serde_json::json!({"path": "."}),
                }],
            },
        };
        let out = to_new_message(&msg, 42);
        assert_eq!(out.content, "");
    }

    #[test]
    fn tool_message_maps_correctly() {
        let msg = AgentMessage::Tool {
            tool_call_id: "call_1".into(),
            content: "file contents here".into(),
        };
        let out = to_new_message(&msg, 42);
        assert_eq!(out.role, MessageRole::Tool);
        assert_eq!(out.content, "file contents here");
        let meta = out.metadata.unwrap();
        assert_eq!(meta["tool_call_id"], "call_1");
    }
}
