//! What a client sends to change a chat, what it is told back, and how a
//! chat is read with its branch points.

use serde::{Deserialize, Serialize};

use crate::domain::attachment::AttachmentId;
use crate::domain::chat::{Message, MessageRole};

/// A change to a saved chat. One that would discard or alter a saved reply
/// branches the chat into a new identical one and changes that instead
/// ([`plan()`](super::plan())).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatChange {
    /// New text for a message: a question, asked again with it, or a reply,
    /// kept as written.
    Edit {
        /// The message edited.
        message_id: i64,
        /// Its new text.
        content: String,
        /// A question's images, by id, in order. A reply carries none.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<AttachmentId>,
    },
    /// The reply holding this message answered again.
    Regenerate {
        /// A message of the reply.
        message_id: i64,
    },
    /// The chat copied, as far as the end of the turn holding this message,
    /// into a new chat to carry on from there.
    Branch {
        /// A message of the turn.
        message_id: i64,
    },
}

/// What a change did: the chat it left to be shown, whether that is a new
/// branch, and whether its last question is now to be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatChanged {
    /// The chat to show: the new branch, or the chat changed in place.
    pub conversation_id: i64,
    /// Whether the change made a new chat and left the one it was made on
    /// as it was.
    pub forked: bool,
    /// Whether the chat's last message is a question its client is now to
    /// have answered.
    pub answer: bool,
}

/// A chat as a client reads it: its messages, the branch points along them,
/// and whether its last message is a question that has no answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatThread {
    /// Every saved message, oldest first, each with the metadata it was saved
    /// with and the images it carries, without their bytes.
    pub messages: Vec<Message>,
    /// The points along `messages` where the chat's family holds other
    /// options. Left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<BranchPoint>,
    /// Whether the last message is a question with no reply: a Retry answers
    /// it. Left out when it is not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub answerable: bool,
}

/// A point where the chat's family holds two or more different turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchPoint {
    /// This chat's message that starts its turn at this point, or `None`
    /// for the point after its last message, where another branch goes on.
    pub message_id: Option<i64>,
    /// Which of `options` this chat is.
    pub index: u32,
    /// Every option, oldest first.
    pub options: Vec<BranchOption>,
}

/// One option at a branch point, as a list of them shows it. Choosing it
/// opens the chat that holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchOption {
    /// The chat that holds this option.
    pub conversation_id: i64,
    /// Its message at this point; `None` for this chat's own empty option,
    /// a branch that has nothing here yet.
    pub message_id: Option<i64>,
    /// Whose turn it is; `None` with `message_id`.
    pub role: Option<MessageRole>,
    /// The line it is shown by ([`preview()`](super::preview())); empty with
    /// `message_id`.
    pub preview: String,
}
