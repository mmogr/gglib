//! When a change branches a chat, and what it then writes.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::ChatChange;
use super::units::{Unit, units};
use crate::domain::attachment::AttachmentId;
use crate::domain::chat::{Message, MessageRole};

/// What a change writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    /// The chat's last question, which nothing answers, is replaced by the
    /// edited one, and the chat is answered.
    Replace {
        /// The question replaced.
        question: i64,
    },
    /// A new chat of the same family holds a copy of every message as far as
    /// `through` (none for `None`), then `then`. The chat the change was made
    /// on is left as it was.
    Fork {
        /// The last message copied.
        through: Option<i64>,
        /// What the new chat holds after the copy.
        then: Then,
        /// Whether the new chat's last question is then answered.
        answer: bool,
    },
}

/// What a new branch holds after the messages it copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Then {
    /// Nothing more.
    Nothing,
    /// The edited question.
    Question,
    /// The edited reply, one assistant message, in place of every message of
    /// the reply it edits.
    EditedReply,
}

/// A change the rules refuse. Nothing is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Refused {
    /// No message of the chat has that id.
    #[error("message {0} is not in this conversation")]
    MessageNotFound(i64),
    /// The edit says what the message says.
    #[error("the edit changes nothing")]
    Unchanged,
    /// Only a reply is answered again; a question is asked again by Retry.
    #[error("message {0} is a question; only a reply is regenerated")]
    NotAReply(i64),
    /// The chat ends in no question to answer.
    #[error("the conversation ends in no question to answer")]
    NothingToAnswer,
    /// A reply carries no image.
    #[error("an edited reply carries no image")]
    ImagesOnReply,
}

impl Refused {
    /// The code a client matches on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MessageNotFound(_) => "message_not_found",
            Self::Unchanged => "unchanged",
            Self::NotAReply(_) => "not_a_reply",
            Self::NothingToAnswer => "nothing_to_answer",
            Self::ImagesOnReply => "invalid_request",
        }
    }
}

fn roles(path: &[Message]) -> Vec<MessageRole> {
    path.iter().map(|message| message.role).collect()
}

/// What `change` writes to a chat whose messages are `path`, `busy` saying
/// whether a reply to it is being written.
///
/// A saved reply is never discarded or altered: a change that would do
/// either forks. The one change made in place is an edit of the chat's last
/// question while nothing answers it, nor is being written.
///
/// # Errors
///
/// [`Refused`], for a change that names no message of the chat, changes
/// nothing, regenerates a question, regenerates a reply that answers no
/// question, or gives a reply images.
pub fn plan(path: &[Message], change: &ChatChange, busy: bool) -> Result<Plan, Refused> {
    let id = match change {
        ChatChange::Edit { message_id, .. }
        | ChatChange::Regenerate { message_id }
        | ChatChange::Branch { message_id } => *message_id,
    };
    let at = path
        .iter()
        .position(|message| message.id == id)
        .ok_or(Refused::MessageNotFound(id))?;
    let units = units(&roles(path));
    let held = units
        .iter()
        .position(|unit| unit.holds(at))
        .ok_or(Refused::MessageNotFound(id))?;
    let unit = &units[held];
    let before = |start: usize| start.checked_sub(1).map(|i| path[i].id);
    match (change, unit) {
        (
            ChatChange::Edit {
                content, images, ..
            },
            Unit::Question(q),
        ) => {
            if !differs(&path[*q], content, images) {
                Err(Refused::Unchanged)
            } else if held + 1 == units.len() && !busy {
                Ok(Plan::Replace { question: id })
            } else {
                Ok(Plan::Fork {
                    through: before(*q),
                    then: Then::Question,
                    answer: true,
                })
            }
        }
        (
            ChatChange::Edit {
                content, images, ..
            },
            Unit::Reply(rows),
        ) => {
            if !images.is_empty() {
                Err(Refused::ImagesOnReply)
            } else if *content == path[at].content {
                Err(Refused::Unchanged)
            } else {
                Ok(Plan::Fork {
                    through: before(rows.start),
                    then: Then::EditedReply,
                    answer: false,
                })
            }
        }
        (ChatChange::Regenerate { .. }, Unit::Question(_)) => Err(Refused::NotAReply(id)),
        (ChatChange::Regenerate { .. }, Unit::Reply(rows)) => {
            let asked = held > 0 && matches!(units[held - 1], Unit::Question(_));
            if !asked {
                return Err(Refused::NothingToAnswer);
            }
            Ok(Plan::Fork {
                through: before(rows.start),
                then: Then::Nothing,
                answer: true,
            })
        }
        (ChatChange::Branch { .. }, unit) => Ok(Plan::Fork {
            through: Some(path[unit.last()].id),
            then: Then::Nothing,
            answer: false,
        }),
    }
}

fn differs(question: &Message, content: &str, images: &[AttachmentId]) -> bool {
    let held: Vec<&AttachmentId> = question.images.iter().map(|image| &image.id).collect();
    question.content != content || !held.iter().copied().eq(images.iter())
}

/// Whether a chat whose messages are `path` ends in a question with no
/// reply, which an answer run then answers.
///
/// # Errors
///
/// [`Refused::NothingToAnswer`] when its last turn is a reply, or it has
/// none.
pub fn answerable(path: &[Message]) -> Result<(), Refused> {
    match units(&roles(path)).last() {
        Some(Unit::Question(_)) => Ok(()),
        _ => Err(Refused::NothingToAnswer),
    }
}
