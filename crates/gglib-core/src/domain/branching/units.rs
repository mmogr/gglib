//! A chat's messages as turns: a question, or a reply.

use std::ops::Range;

use crate::domain::chat::MessageRole;

/// One turn of a chat, by its messages' positions.
///
/// A question is one user message; a reply is the longest run of assistant
/// and tool messages after one. A system message is no part of a turn: a
/// chat's prompt is a setting of the chat, never a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unit {
    /// A user's message.
    Question(usize),
    /// The messages of one reply.
    Reply(Range<usize>),
}

impl Unit {
    /// The position the turn starts at.
    #[must_use]
    pub const fn start(&self) -> usize {
        match self {
            Self::Question(at) => *at,
            Self::Reply(rows) => rows.start,
        }
    }

    /// The position of the turn's last message.
    #[must_use]
    pub const fn last(&self) -> usize {
        match self {
            Self::Question(at) => *at,
            Self::Reply(rows) => rows.end - 1,
        }
    }

    /// Whether the message at `at` is one of the turn's.
    #[must_use]
    pub fn holds(&self, at: usize) -> bool {
        match self {
            Self::Question(question) => *question == at,
            Self::Reply(rows) => rows.contains(&at),
        }
    }
}

/// The turns of messages with these `roles`, in order.
#[must_use]
pub fn units(roles: &[MessageRole]) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    for (at, role) in roles.iter().enumerate() {
        match role {
            MessageRole::System => {}
            MessageRole::User => units.push(Unit::Question(at)),
            MessageRole::Assistant | MessageRole::Tool => match units.last_mut() {
                Some(Unit::Reply(rows)) => rows.end = at + 1,
                _ => units.push(Unit::Reply(at..at + 1)),
            },
        }
    }
    units
}
