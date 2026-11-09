//! The hub's chats as a paired device reads them, and the turn it adds.
//!
//! "Look, don't copy": a device reads the hub's own rows, live, and never a
//! copy it keeps. An optional field is left out of the body when it has no
//! value, as a run's are.

use serde::{Deserialize, Serialize};

use super::attachment::AttachmentId;
use super::branching::BranchPoint;
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
    /// The chat it was branched from (ADR 0017), when it is a branch.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_of: Option<i64>,
}

/// The hub's chats, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HubChatList {
    /// Each chat, the most recently changed first.
    pub chats: Vec<HubChat>,
}

/// One chat opened: the conversation, every row of it, in order, and what
/// the hub says of its branches (ADR 0017).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HubChatOpen {
    /// The conversation.
    pub conversation: Conversation,
    /// Its rows, oldest first, each with the metadata the hub saved and the
    /// images it carries, without their bytes: a device reads an image by
    /// its id, at `GET /v1/attachments/{id}`.
    pub messages: Vec<Message>,
    /// The points along `messages` where the chat's family holds other
    /// options, each opened as a chat of its own. Left out when there are
    /// none.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<Vec<BranchPoint>>", optional))]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<BranchPoint>,
    /// Whether the last row is a question with no reply, which a turn that
    /// says `answer_saved` answers. Left out when it is not.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<bool>", optional))]
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub answerable: bool,
}

/// A turn a paired device adds to one of the hub's chats.
///
/// The body of `PUT /v1/runs/{id}?kind=agent` on the proxy's door. No
/// history travels: the hub rebuilds it from its own record. No image
/// travels either: the device uploads each one first, at
/// `POST /v1/attachments`, and names it here by its id. A turn that says
/// `answer_saved` adds no message: it answers the question the chat already
/// ends in, as an edit or a regenerate leaves it. Any other key is refused,
/// so a client sending `model` or `messages` learns none is honoured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HubTurn {
    /// The hub's chat the turn is added to.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub conversation_id: i64,
    /// The user's message. Empty when the turn is its images alone, and
    /// when it answers the question already saved.
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
    /// Whether the turn answers the question the chat ends in, already
    /// saved, rather than adding a message: it then carries no text and no
    /// image, and only its reply is saved. Left out of the body when false.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<bool>", optional))]
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub answer_saved: bool,
    /// The message was sent with Draw pressed: the hub offers its model the
    /// image tool for this turn, and only then. Left out of the body when
    /// `false`, so a hub too old to know the key is never sent it by a
    /// client that did not press Draw; a client asks
    /// `GET /v1/images/drawing` before it offers the button.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<bool>", optional))]
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draw: bool,
    /// The image model the person chose for this turn's picture, by the
    /// `id` of its row in the hub's `GET /v1/images/drawing` answer. Only
    /// with `draw`; left out, the hub's default rule draws. Nothing of it
    /// is remembered by the hub: a client that keeps the choice says it on
    /// every turn that draws. Left out of the body when unsaid, so only a
    /// hub whose answer listed `models` is ever sent it.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_model: Option<u32>,
}

/// A turn this machine's own page adds to one of its chats.
///
/// The body of `PUT /api/runs/{id}?kind=agent` on the daemon's door: a
/// [`HubTurn`], key for key, and the chat's folder, which only this
/// machine's turn may say (ADR 0023). The daemon rebuilds the history from
/// its record, and runs the turn on the model, tools and limits the chat's
/// settings save. Any other key is refused, so a page sending `messages`,
/// `port` or `tool_filter` learns none is honoured.
///
/// Its fields are [`HubTurn`]'s written out, not flattened: a flattened
/// struct cannot refuse an unknown key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct LocalTurn {
    /// The chat the turn is added to.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub conversation_id: i64,
    /// The user's message. Empty when the turn is its images alone, and
    /// when it answers the question already saved.
    pub content: String,
    /// The images the message carries, by the ids their uploads answered,
    /// in order. Left out of the body when there are none.
    #[cfg_attr(feature = "ts-bindings", ts(type = "Array<string>", optional))]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<AttachmentId>,
    /// The chat's Thinking choice, said only on the turn that changes it,
    /// as [`HubTurn::thinking`].
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    /// Whether the turn answers the question the chat ends in, already
    /// saved, as an edit, a regenerate or Retry leaves it, rather than
    /// adding a message. Left out of the body when false.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<bool>", optional))]
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub answer_saved: bool,
    /// The message was sent with Draw pressed, as [`HubTurn::draw`].
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<bool>", optional))]
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draw: bool,
    /// The image model chosen for this turn's picture, as
    /// [`HubTurn::image_model`]: only with `draw`, and remembered nowhere.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_model: Option<u32>,
    /// The chat's folder, where gglib's own file tools read, said only on
    /// the turn that changes it: an absolute path to a folder on this
    /// machine sets it, and `""` forgets it. Left out, the turn reads the
    /// folder the chat remembers, or has no file tools.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

impl LocalTurn {
    /// The turn as a [`HubTurn`], and the folder it says.
    #[must_use]
    pub fn split(self) -> (HubTurn, Option<String>) {
        let Self {
            conversation_id,
            content,
            images,
            thinking,
            answer_saved,
            draw,
            image_model,
            folder,
        } = self;
        let turn = HubTurn {
            conversation_id,
            content,
            images,
            thinking,
            answer_saved,
            draw,
            image_model,
        };
        (turn, folder)
    }
}

#[cfg(test)]
#[path = "hub_chats_branch_tests.rs"]
mod hub_chats_branch_tests;
#[cfg(test)]
#[path = "hub_chats_local_tests.rs"]
mod hub_chats_local_tests;
#[cfg(test)]
#[path = "hub_chats_tests.rs"]
mod hub_chats_tests;
#[cfg(test)]
#[path = "hub_chats_thinking_tests.rs"]
mod hub_chats_thinking_tests;
