//! What one line typed at the REPL's prompt asks for.
//!
//! The prompt reads a line and this says what it is: a command, or the
//! next message with the images attached to it. The loop in
//! [`repl`](super::repl) does what it says.

use gglib_core::domain::agent::AgentMessage;

use super::branches::Ask;
use super::images::TurnImages;

/// A line typed at the prompt, trimmed.
#[derive(Debug)]
pub(super) enum Line {
    /// Nothing was typed.
    Empty,
    /// `/quit` or `/exit`.
    Quit,
    /// `/help`.
    Help,
    /// `/retry`, `/edit <text>` or `/branch`: a change to the session's
    /// chat ([`branches`](super::branches)).
    Change(Ask),
    /// `/branches`: the branch points along the session's chat.
    Branches,
    /// `/image`: what to tell the user, be it the receipt of the image now
    /// attached to the next message, the usage, or why it was refused.
    Image(String),
    /// `/draw`: let the model draw for the next message.
    Draw,
    /// Anything else is the user's next message. It carries every image
    /// attached since the last one, and none waits after it.
    Send(AgentMessage),
}

/// Read `input`, a trimmed line, as a [`Line`]. `images` gains the file an
/// `/image` line names, and gives the message its images.
pub(super) async fn read(input: &str, images: &mut TurnImages<'_>) -> Line {
    match input {
        "" => return Line::Empty,
        "/quit" | "/exit" => return Line::Quit,
        "/help" => return Line::Help,
        "/retry" => return Line::Change(Ask::Retry),
        "/branch" => return Line::Change(Ask::Branch),
        "/branches" => return Line::Branches,
        "/draw" => return Line::Draw,
        _ => {}
    }
    if let Some(text) = input.strip_prefix("/edit")
        && (text.is_empty() || text.starts_with(char::is_whitespace))
    {
        return Line::Change(Ask::Edit(text.trim().to_owned()));
    }
    if let Some(reply) = images.command(input).await {
        return Line::Image(reply);
    }
    Line::Send(AgentMessage::User {
        content: input.to_owned(),
        images: images.take(),
    })
}

#[cfg(test)]
#[path = "repl_line_tests.rs"]
mod repl_line_tests;
