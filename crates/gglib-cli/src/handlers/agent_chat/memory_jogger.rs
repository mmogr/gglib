//! Showing the user where a resumed chat left off: the last exchange,
//! reprinted so the first new turn has visible context.

use std::fmt::Write as _;

use gglib_core::domain::attachment::AttachmentInfo;
use gglib_core::domain::chat::{Message, MessageRole};

use super::images;
use super::pictures::Links;
use crate::presentation::style;

/// Where the last exchange's tool messages start: after its user message.
fn since_last_user(db_messages: &[Message]) -> usize {
    db_messages
        .iter()
        .rposition(|m| m.role == MessageRole::User)
        .map_or(0, |at| at + 1)
}

/// The images [`memory_jogger`] shows a marker for: the last user
/// message's, and those the tools made after it.
pub(crate) fn shown_images(db_messages: &[Message]) -> Vec<AttachmentInfo> {
    let since_user = since_last_user(db_messages);
    let users = since_user.checked_sub(1).map(|at| &db_messages[at]);
    let tools = db_messages[since_user..]
        .iter()
        .filter(|m| m.role == MessageRole::Tool);
    users
        .into_iter()
        .chain(tools)
        .flat_map(|m| m.images.iter().cloned())
        .collect()
}

/// The last user/assistant exchange, as a memory jogger for a resumed
/// chat. A user message's images are shown as markers after its text, and
/// the images the exchange's tools made on a line of their own; a marker
/// with a link in `links` opens its image.
pub(crate) fn memory_jogger(db_messages: &[Message], title: &str, links: &Links) -> String {
    let last = |role: MessageRole| db_messages.iter().rev().find(|m| m.role == role);
    let clipped = |content: &str| match content.char_indices().nth(200) {
        Some((end, _)) => format!("{}…", &content[..end]),
        None => content.to_owned(),
    };

    let mut jogger = format!("\n{}Resuming: {}{}\n\n", style::INFO, title, style::RESET);
    if let Some(user) = last(MessageRole::User) {
        let (content, images) = (clipped(&user.content), images::markers(&user.images, links));
        let _ = writeln!(
            jogger,
            "{}  You: {content}{images}{}",
            style::DIM,
            style::RESET
        );
    }
    let since_user = since_last_user(db_messages);
    let tool_images: String = db_messages[since_user..]
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .map(|m| images::markers(&m.images, links))
        .collect();
    if !tool_images.is_empty() {
        let _ = writeln!(
            jogger,
            "{}  Tool images:{tool_images}{}",
            style::DIM,
            style::RESET
        );
    }
    if let Some(assistant) = last(MessageRole::Assistant) {
        let content = clipped(&assistant.content);
        let _ = writeln!(
            jogger,
            "{}  Assistant: {content}{}",
            style::DIM,
            style::RESET
        );
    }
    jogger.push('\n');
    jogger
}

#[cfg(test)]
#[path = "memory_jogger_tests.rs"]
mod memory_jogger_tests;
