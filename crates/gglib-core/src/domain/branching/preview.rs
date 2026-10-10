//! The line an option at a branch point is shown by.

use crate::domain::chat::MessageRole;

/// How many characters of a line an option shows.
pub const PREVIEW_CHARS: usize = 80;

/// A message as a preview reads it: whose it is, its text and how many
/// images it carries.
#[derive(Debug, Clone, Copy)]
pub struct PreviewRow<'a> {
    /// Whose message it is.
    pub role: MessageRole,
    /// Its text.
    pub text: &'a str,
    /// How many images it carries.
    pub images: usize,
}

/// The line a turn is shown by, from its messages in order.
///
/// A question is shown by its text, a reply by its last message of the
/// model's own that has some: never by a tool's result. The line is the
/// text's first that is not blank, trimmed, cut at [`PREVIEW_CHARS`]
/// characters with `…` after. A question of images alone is called what it
/// holds, as ggchat calls such a chat; a reply with no text of its own reads
/// `(no text)`.
#[must_use]
pub fn preview(turn: &[PreviewRow<'_>]) -> String {
    let Some(first) = turn.first() else {
        return String::new();
    };
    if first.role == MessageRole::User {
        let line = first_line(first.text);
        return match (line.is_empty(), first.images) {
            (false, _) | (true, 0) => line,
            (true, 1) => "An image".to_owned(),
            (true, n) => format!("{n} images"),
        };
    }
    turn.iter()
        .rev()
        .filter(|row| row.role == MessageRole::Assistant)
        .map(|row| first_line(row.text))
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| "(no text)".to_owned())
}

fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let mut chars = line.chars();
    let shown: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        format!("{shown}…")
    } else {
        shown
    }
}
