//! A chat as an export writes it to a file (`ChatExport`).
//!
//! The file holds the conversation and every saved message, each image's
//! bytes beside its info as a `data:` URL, so it shows what was asked about
//! on a machine whose store never held the image.
//!
//! The one shape of the file: the chat page's Export and `gglib chat export`
//! both write what [`AppCore::export_chat`](crate::services::AppCore::export_chat)
//! answers.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::attachment::{AttachmentId, AttachmentInfo};
use super::chat::{Conversation, Message, MessageRole};

/// A conversation and its saved messages, its images' bytes in it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ChatExport {
    /// The conversation, as `GET /api/conversations` lists it.
    pub conversation: Conversation,
    /// Every saved message, oldest first.
    pub messages: Vec<ExportedMessage>,
}

/// A saved message as the file holds it: a [`Message`], each image with its
/// bytes when the store gave them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ExportedMessage {
    /// The message's id.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
    /// The conversation it is in.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub conversation_id: i64,
    /// Who wrote it.
    pub role: MessageRole,
    /// Its text.
    pub content: String,
    /// When it was saved.
    pub created_at: String,
    /// The metadata it was saved with: tool calls, thinking, figures.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(type = "Record<string, unknown>", optional)
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// The images it carries, in order. Left out when there are none.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<Vec<ExportedImage>>", optional)
    )]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ExportedImage>,
}

/// An image in the file: its info, and its bytes when the store gave them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ExportedImage {
    /// What is known of it without its bytes.
    #[serde(flatten)]
    pub info: AttachmentInfo,
    /// `data:<mime>;base64,<bytes>`. Left out when the store could not give
    /// the bytes: the image is then in the file by its id alone.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
}

impl ChatExport {
    /// `conversation` and `messages`, each image beside the `data:` URL
    /// `urls` holds for its id, or by its info alone when it holds none.
    #[must_use]
    pub fn new(
        conversation: Conversation,
        messages: Vec<Message>,
        urls: &HashMap<AttachmentId, String>,
    ) -> Self {
        let messages = messages
            .into_iter()
            .map(|message| ExportedMessage {
                id: message.id,
                conversation_id: message.conversation_id,
                role: message.role,
                content: message.content,
                created_at: message.created_at,
                metadata: message.metadata,
                images: message
                    .images
                    .into_iter()
                    .map(|info| ExportedImage {
                        data_url: urls.get(&info.id).cloned(),
                        info,
                    })
                    .collect(),
            })
            .collect();
        Self {
            conversation,
            messages,
        }
    }

    /// The images in the file by their id alone, each counted once however
    /// many messages carry it.
    #[must_use]
    pub fn unread(&self) -> usize {
        self.messages
            .iter()
            .flat_map(|message| &message.images)
            .filter(|image| image.data_url.is_none())
            .map(|image| &image.info.id)
            .collect::<HashSet<_>>()
            .len()
    }

    /// The name the file is saved by: `conversation-<id>.json`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("conversation-{}.json", self.conversation.id)
    }

    /// The file's text: the export as indented JSON, two spaces a level.
    /// The page and the command line both save this text as it is.
    ///
    /// # Panics
    ///
    /// Never: every value in an export serializes.
    #[must_use]
    pub fn text(&self) -> String {
        serde_json::to_string_pretty(self).expect("an export serializes")
    }
}

/// What a client tells someone of the images that are in the file by their
/// id only; `None` when every image's bytes are in it.
#[must_use]
pub fn unread_sentence(unread: usize) -> Option<String> {
    match unread {
        0 => None,
        1 => Some("1 image could not be read. It is in the file by its id only.".to_owned()),
        n => Some(format!(
            "{n} images could not be read. They are in the file by their ids only."
        )),
    }
}

#[cfg(test)]
#[path = "chat_export_tests.rs"]
mod chat_export_tests;
