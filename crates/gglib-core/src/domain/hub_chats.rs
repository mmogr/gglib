//! The hub's chats as a paired device reads them, and the turn it adds.
//!
//! "Look, don't copy": a device reads the hub's own rows, live, and never a
//! copy it keeps. An optional field is left out of the body when it has no
//! value, as a run's are.

use serde::{Deserialize, Serialize};

use super::attachment::AttachmentId;
use super::chat::{Conversation, Message};
use super::thinking::Thinking;

/// One of the hub's chats, as `GET /v1/chats` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HubChat {
    /// The conversation's id on the hub.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
    /// Its title.
    pub title: String,
    /// The catalogue model it was made with, when it names one.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<i64>,
    /// That model's name, when the catalogue still has it.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// When it last changed, as the hub's database writes it.
    pub updated_at: String,
    /// The run whose reply to it is not yet saved, if one is.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_run: Option<String>,
}

/// The hub's chats, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HubChatList {
    /// Each chat, the most recently changed first.
    pub chats: Vec<HubChat>,
}

/// One chat opened: the conversation and every row of it, in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HubChatOpen {
    /// The conversation.
    pub conversation: Conversation,
    /// Its rows, oldest first, each with the metadata the hub saved and the
    /// images it carries, without their bytes: a device reads an image by
    /// its id, at `GET /v1/attachments/{id}`.
    pub messages: Vec<Message>,
}

/// A turn a paired device adds to one of the hub's chats.
///
/// The body of `PUT /v1/runs/{id}?kind=agent` on the proxy's door. No
/// history travels: the hub rebuilds it from its own record. No image
/// travels either: the device uploads each one first, at
/// `POST /v1/attachments`, and names it here by its id. Any other key is
/// refused, so a client sending `model`, `messages` or `replace_from` learns
/// none is honoured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HubTurn {
    /// The hub's chat the turn is added to.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub conversation_id: i64,
    /// The user's message. Empty when the turn is its images alone.
    pub content: String,
    /// The images the message carries, by id, in order. Left out of the
    /// body when there are none.
    #[cfg_attr(feature = "ts-bindings", ts(type = "Array<string>", optional))]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<AttachmentId>,
    /// The chat's Thinking choice, said only on the turn that changes it:
    /// `off` runs this turn and the chat's later ones with a thinking
    /// budget of `0`, `default` forgets that. Left out of the body when
    /// unsaid, and the turn then runs as the chat remembers.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
}

#[cfg(test)]
#[path = "hub_chats_tests.rs"]
mod hub_chats_tests;
#[cfg(test)]
#[path = "hub_chats_thinking_tests.rs"]
mod hub_chats_thinking_tests;
