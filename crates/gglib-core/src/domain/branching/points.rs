//! The options a chat's family holds at each turn of the chat.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::preview::{PreviewRow, preview};
use super::units::units;
use super::{BranchOption, BranchPoint};
use crate::domain::chat::MessageRole;

/// One message of a chat in a family, as the branch points read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRow {
    /// The message's id.
    pub id: i64,
    /// The id of the message it is a copy of, or its own: equal keys at a
    /// position mean equal chats up to there.
    pub key: i64,
    /// Whose message it is.
    pub role: MessageRole,
    /// Its text, or as much of it as a preview needs.
    pub text: String,
    /// How many images it carries.
    pub images: usize,
}

/// One chat of a family: the chats branched from one another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineChat {
    /// The chat's id.
    pub conversation_id: i64,
    /// When it last changed, as the database writes it.
    pub updated_at: String,
    /// Its messages but system ones, oldest first.
    pub rows: Vec<LineRow>,
}

/// The branch points along `me`, one of `family`'s chats.
///
/// At each turn of `me`, and after its last, the family's chats that hold
/// what `me` holds up to there are compared: those that go on with a turn
/// there are the options, one per different turn, oldest first. `me` stands
/// for its own option; another is shown by the chat that holds it and
/// changed last. After its last message, `me` is an empty option when
/// another chat goes on there. A point with one option is no point.
#[must_use]
pub fn points(me: i64, family: &[LineChat]) -> Vec<BranchPoint> {
    let Some(mine) = family.iter().find(|chat| chat.conversation_id == me) else {
        return Vec::new();
    };
    let roles: Vec<MessageRole> = mine.rows.iter().map(|row| row.role).collect();
    units(&roles)
        .iter()
        .map(super::Unit::start)
        .chain([mine.rows.len()])
        .filter_map(|at| point(mine, family, at))
        .collect()
}

fn point(mine: &LineChat, family: &[LineChat], at: usize) -> Option<BranchPoint> {
    let shares = |chat: &LineChat| {
        at == 0
            || chat
                .rows
                .get(at - 1)
                .is_some_and(|row| row.key == mine.rows[at - 1].key)
    };
    // Each different turn at `at`, by key, and the chat that shows it.
    let mut turns: BTreeMap<i64, &LineChat> = BTreeMap::new();
    for chat in family.iter().filter(|chat| shares(chat)) {
        if !starts_turn(&chat.rows, at) {
            continue;
        }
        let key = chat.rows[at].key;
        let shown = turns.entry(key).or_insert(chat);
        if shown.conversation_id != mine.conversation_id
            && (chat.conversation_id == mine.conversation_id || newer(chat, shown))
        {
            *shown = chat;
        }
    }
    let empty = at == mine.rows.len();
    if turns.len() + usize::from(empty) < 2 {
        return None;
    }
    let mut options: Vec<BranchOption> = turns.values().map(|chat| option(chat, at)).collect();
    if empty {
        options.push(BranchOption {
            conversation_id: mine.conversation_id,
            message_id: None,
            role: None,
            preview: String::new(),
        });
    }
    let index = options
        .iter()
        .position(|option| option.conversation_id == mine.conversation_id)?;
    Some(BranchPoint {
        message_id: mine.rows.get(at).map(|row| row.id),
        index: u32::try_from(index).unwrap_or(u32::MAX),
        options,
    })
}

/// Whether `rows` has a turn starting at `at`: a question, or a reply after
/// one.
fn starts_turn(rows: &[LineRow], at: usize) -> bool {
    let Some(row) = rows.get(at) else {
        return false;
    };
    row.role == MessageRole::User || at == 0 || rows[at - 1].role == MessageRole::User
}

fn newer(chat: &LineChat, than: &LineChat) -> bool {
    let rank = |c: &LineChat| (c.updated_at.clone(), c.conversation_id);
    rank(chat) > rank(than)
}

fn option(chat: &LineChat, at: usize) -> BranchOption {
    let row = &chat.rows[at];
    let turn: Vec<PreviewRow<'_>> = chat.rows[at..]
        .iter()
        .enumerate()
        .take_while(|(i, r)| {
            *i == 0 || (row.role != MessageRole::User && r.role != MessageRole::User)
        })
        .map(|(_, r)| PreviewRow {
            role: r.role,
            text: &r.text,
            images: r.images,
        })
        .collect();
    BranchOption {
        conversation_id: chat.conversation_id,
        message_id: Some(row.id),
        role: Some(row.role),
        preview: preview(&turn),
    }
}
