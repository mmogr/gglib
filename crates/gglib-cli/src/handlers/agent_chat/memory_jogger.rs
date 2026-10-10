//! Showing the user where a resumed chat left off: the last exchange,
//! reprinted so the first new turn has visible context.

use std::fmt::Write as _;

use gglib_core::domain::chat::{Message, MessageRole};

use super::images;
use crate::presentation::style;

/// The last user/assistant exchange, as a memory jogger for a resumed
/// chat. A user message's images are shown as markers after its text, and
/// the images the exchange's tools made on a line of their own.
pub(crate) fn memory_jogger(db_messages: &[Message], title: &str) -> String {
    let last = |role: MessageRole| db_messages.iter().rev().find(|m| m.role == role);
    let clipped = |content: &str| match content.char_indices().nth(200) {
        Some((end, _)) => format!("{}…", &content[..end]),
        None => content.to_owned(),
    };

    let mut jogger = format!("\n{}Resuming: {}{}\n\n", style::INFO, title, style::RESET);
    if let Some(user) = last(MessageRole::User) {
        let (content, images) = (clipped(&user.content), images::markers(&user.images));
        let _ = writeln!(
            jogger,
            "{}  You: {content}{images}{}",
            style::DIM,
            style::RESET
        );
    }
    let since_user = db_messages
        .iter()
        .rposition(|m| m.role == MessageRole::User)
        .map_or(0, |at| at + 1);
    let tool_images: String = db_messages[since_user..]
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .map(|m| images::markers(&m.images))
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
