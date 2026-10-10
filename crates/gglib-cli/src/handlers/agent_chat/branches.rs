//! What `/retry`, `/edit`, `/branch` and `/branches` do to a session's saved
//! chat (ADR 0017).
//!
//! Each change is the chat history service's to make, as the branching
//! rules say: one that would discard or alter a saved reply is made on a new
//! branch of the chat, a copy as far as the change, and the chat it was made
//! on is kept as it was. Only an edit of the last question, while nothing
//! answers it, is made in place. The session then goes on in the chat the
//! change leaves, from its saved history, and answers it when the change
//! says so.
//!
//! The session's chat is its own: `--continue` is refused while the daemon
//! is replying to it, so a change here is never made under a reply being
//! written.

use std::fmt::Write as _;

use gglib_core::domain::agent::{AgentMessage, saved_history};
use gglib_core::domain::branching::{self, ChatChange, ChatThread};
use gglib_core::domain::chat::{Message, MessageRole};

use super::persistence::Conversation;

/// What `/edit` says when it names no text.
pub(super) const EDIT_USAGE: &str = "Usage: /edit <the last question, asked again>";

/// A change typed at the prompt.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Ask {
    /// `/retry`: answer the question the chat ends in, or, when it ends in
    /// a reply, answer that reply's question again on a new branch.
    Retry,
    /// `/edit <text>`: the last question asked again as `text`, with the
    /// images it carries.
    Edit(String),
    /// `/branch`: the whole chat copied into a new branch to go on in.
    Branch,
}

/// Where a change leaves the session.
pub(super) struct Next {
    /// The history it goes on from: the session's prompt, then every saved
    /// message of the chat it is now in.
    pub history: Vec<AgentMessage>,
    /// Whether that chat ends in a question it is now to answer.
    pub answer: bool,
}

/// The change `ask` asks of a chat whose messages are `rows`, or why there
/// is none to make. `None` with no reason is Retry on a chat that ends in a
/// question: nothing changes, and the question is answered.
fn change_for(ask: Ask, rows: &[Message]) -> Result<Option<ChatChange>, &'static str> {
    let last = |role: Option<MessageRole>| {
        rows.iter()
            .rev()
            .find(|row| role.is_none_or(|role| row.role == role))
    };
    match ask {
        Ask::Retry if branching::answerable(rows).is_ok() => Ok(None),
        Ask::Retry => last(None)
            .map(|row| Some(ChatChange::Regenerate { message_id: row.id }))
            .ok_or("Nothing to retry: the chat has no messages yet."),
        Ask::Edit(text) if text.is_empty() => Err(EDIT_USAGE),
        Ask::Edit(content) => last(Some(MessageRole::User))
            .map(|row| {
                Some(ChatChange::Edit {
                    message_id: row.id,
                    content,
                    images: row.images.iter().map(|image| image.id.clone()).collect(),
                })
            })
            .ok_or("Nothing to edit: the chat has no question yet."),
        Ask::Branch => last(None)
            .map(|row| Some(ChatChange::Branch { message_id: row.id }))
            .ok_or("Nothing to branch: the chat has no messages yet."),
    }
}

/// Make `ask` to the session's chat, `persistence`, and move the session to
/// the chat the change leaves. The session's system prompt, which heads
/// `session`, its history, heads the history it goes on from. `None` when
/// nothing was made; why is said on stderr.
pub(super) async fn go(
    persistence: &mut Option<Conversation<'_>>,
    ask: Ask,
    session: &[AgentMessage],
) -> Option<Next> {
    let prompt = match session.first() {
        Some(AgentMessage::System { content }) => Some(content.as_str()),
        _ => None,
    };
    let Some(conversation) = persistence.as_ref() else {
        eprintln!("This session is not saved, so it is not changed: send your next message.");
        return None;
    };
    let history = conversation.history();
    let rows = match history.get_messages(conversation.id).await {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("The chat could not be read: {e}");
            return None;
        }
    };
    let made = match change_for(ask, &rows) {
        Ok(made) => made,
        Err(why) => {
            eprintln!("{why}");
            return None;
        }
    };
    let (target, answer) = match made {
        None => (conversation.id, true),
        Some(change) => match history.change(conversation.id, &change, false).await {
            Ok(changed) => {
                if changed.forked {
                    eprintln!(
                        "Saved as a new branch, chat #{}. Chat #{} is kept as it was.",
                        changed.conversation_id, conversation.id
                    );
                }
                (changed.conversation_id, changed.answer)
            }
            Err(e) => {
                eprintln!("Nothing was changed: {e}");
                return None;
            }
        },
    };
    *persistence = Some(conversation.moved_to(target));
    match history.get_messages(target).await {
        Ok(rows) => Some(Next {
            history: saved_history(prompt, &rows),
            answer,
        }),
        Err(e) => {
            eprintln!("The chat could not be read: {e}");
            None
        }
    }
}

/// Print the branch points along the session's chat.
pub(super) async fn list(persistence: Option<&Conversation<'_>>) {
    let Some(conversation) = persistence else {
        eprintln!("This session is not saved, so it has no branches.");
        return;
    };
    match conversation.history().thread(conversation.id).await {
        Ok(thread) => print!("{}", describe(conversation.id, &thread)),
        Err(e) => eprintln!("The chat could not be read: {e}"),
    }
}

/// The branch points along chat `id`, as `/branches` prints them: at each,
/// every option by its chat and its line, this chat's marked.
fn describe(id: i64, thread: &ChatThread) -> String {
    if thread.points.is_empty() {
        return format!("Chat #{id} has no other branches.\n");
    }
    let mut out = format!("Branches along chat #{id}:\n");
    for point in &thread.points {
        let at = point.message_id.map_or_else(
            || "After its last message".to_owned(),
            |message| {
                let turn = match thread.messages.iter().find(|m| m.id == message) {
                    Some(m) if m.role == MessageRole::User => "the question",
                    _ => "the reply",
                };
                format!("At {turn} #{message}")
            },
        );
        let _ = writeln!(out, "  {at}:");
        for (index, option) in point.options.iter().enumerate() {
            let mark = if index == point.index as usize {
                '*'
            } else {
                ' '
            };
            let line = if option.message_id.is_none() {
                "(nothing here yet)"
            } else if option.preview.is_empty() {
                "(no text)"
            } else {
                option.preview.as_str()
            };
            let _ = writeln!(out, "  {mark} #{:<6} {line}", option.conversation_id);
        }
    }
    out.push_str("Open one with: gglib chat --continue <ID>\n");
    out
}

#[cfg(test)]
#[path = "branches_tests.rs"]
mod branches_tests;
