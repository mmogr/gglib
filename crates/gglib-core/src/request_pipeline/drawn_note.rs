//! Stage 1b: a chat that holds a drawn picture tells the model so.
//!
//! A model is offered the drawing tool only for a message sent with Draw.
//! On every other message of that chat it reads a history in which it called
//! a tool it is no longer offered, and is apt to answer that it cannot make
//! images. This stage adds one note, [`DRAWN_NOTE`], to such a request. The
//! note is written into the body on its way to the model: no row, no event
//! and no returned history holds it, and the person never sees it.
//!
//! # When
//!
//! All of these, read from the body as the caller built it:
//!
//! 1. `messages` holds a drawn picture: an assistant message with a tool call
//!    whose `function.name` is exactly [`DRAW_TOOL_NAME`], and a later `tool`
//!    message with that call's id whose text says a picture was drawn the
//!    person can see (it starts with [`DREW_HEAD`] and contains
//!    [`DREW_SEEN`], as the drawing tool writes it). A call that was answered
//!    with an error drew nothing and does not count; a chat whose every draw
//!    failed gets no note.
//! 2. The request is not the closing one of a run sent with Draw: its last
//!    message is not a `user` message that holds [`DRAW_THEN_SAY`] or
//!    [`DRAW_THEN_SAY_FAILED`]. That request already tells the model what to
//!    say. The roles alone do not tell it apart: the message after a draw
//!    whose closing reply never arrived, or whose model wrote its words in
//!    the message that made the call, also ends with a tool's answer and a
//!    `user` message.
//! 3. `tools` does not offer [`DRAW_TOOL_NAME`]. A request that offers it is
//!    the first of a message sent with Draw, and the note's "do not write a
//!    tool call" would contradict the call demanded of it.
//! 4. No message's text already holds the note, so a body shaped twice (a
//!    request to another machine's proxy is) carries it once.
//!
//! Only the builtin's qualified name counts. Another client's own tool named
//! `generate_image`, an MCP server's (`3:generate_image`) and the `/mcp`
//! gateway's `builtin__generate_image` are not a press of Draw, and the note
//! would say something false about them.
//!
//! # Where
//!
//! [`Placement`] names the two places; [`PLACEMENT`] picks one.
//!
//! The stage runs after the reasoning strip and before capability
//! coalescing, so a note placed as a `system` message is treated as any
//! other: a model with no system role reads it as a `[System]:` user
//! message, and a strict-turns model has it merged with the turn beside it.
//! It runs before truncation, so the budget measures the note with the rest
//! of the request.

use serde_json::{Value, json};

use super::content::append_text;

/// The drawing tool's name, as a request's `tools` and a history's
/// `tool_calls` spell it.
pub const DRAW_TOOL_NAME: &str = "builtin:generate_image";

/// What the drawing tool's answer starts with when it drew.
pub const DREW_HEAD: &str = "Drew ";

/// What the drawing tool's answer holds when the person can see what it
/// drew, and never when they cannot.
pub const DREW_SEEN: &str = "the user can see ";

/// What the model is told on the request after the drawing tool drew.
///
/// A model offered no tools is otherwise apt to say it cannot draw. Sent to
/// the model for that request and shown to no one.
pub const DRAW_THEN_SAY: &str = "The picture has been made by the drawing tool and the person is \
    looking at it now. Reply in one or two sentences saying what you asked the tool to draw. \
    Do not apologise, do not say you cannot draw, and do not call a tool.";

/// What the model is told instead when the tool answered with an error.
pub const DRAW_THEN_SAY_FAILED: &str = "The drawing tool did not make a picture; its answer \
    above says why. Tell the person in one or two sentences what went wrong. Do not call a tool.";

/// What the model is told on a later message of a chat that holds a drawn
/// picture.
pub const DRAWN_NOTE: &str = "Earlier in this conversation the person pressed Draw and a \
    drawing tool made a picture from a prompt you wrote. The picture exists and the person can \
    see it; you can talk about what you asked the tool to draw. A new or changed picture is \
    made only when the person presses Draw with their message: only if they ask for a new \
    picture or a change to one, tell them to press Draw and send again. Never say that you \
    cannot make images or that there is no picture, and do not write a tool call.";

/// Where in the prompt the note goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// At the front, as the system prompt: appended to a first message that
    /// is a `system` one, after a blank line, or a `system` message of its
    /// own before everything else.
    SystemFront,
    /// At the start of the first `user` message after the last drawn
    /// picture's `tool` message: before that message's text, or as its first
    /// text part. With no such message nothing is added.
    NextUserMessage,
}

/// The placement every request gets.
///
/// Measured on 2026-10-11 against one 3B model with requests sent straight
/// to llama-server: at the front no reply of nine denied the picture; on the
/// next user message none denied it either, but the model answered the note
/// as if the person had said it.
pub const PLACEMENT: Placement = Placement::SystemFront;

/// Add [`DRAWN_NOTE`] to `body` at [`PLACEMENT`] when the [module](self)'s
/// rule says so. Returns whether `body` changed.
pub(crate) fn note_drawn_picture(body: &mut Value) -> bool {
    place(body, PLACEMENT)
}

/// [`note_drawn_picture`], at a placement the caller names.
pub(crate) fn place(body: &mut Value, placement: Placement) -> bool {
    if offers_drawing(body) {
        return false;
    }
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };
    let Some(drawn_at) = drawn_picture(messages) else {
        return false;
    };
    if awaits_closing_words(messages) {
        return false;
    }
    if messages.iter().any(|message| says(message, DRAWN_NOTE)) {
        return false;
    }
    match placement {
        Placement::SystemFront => {
            at_front(messages);
            true
        }
        Placement::NextUserMessage => on_next_user_message(messages, drawn_at),
    }
}

/// Whether `body` offers the drawing tool.
fn offers_drawing(body: &Value) -> bool {
    let tools = body.get("tools").and_then(Value::as_array);
    tools
        .into_iter()
        .flatten()
        .any(|tool| tool.pointer("/function/name").and_then(Value::as_str) == Some(DRAW_TOOL_NAME))
}

/// Whether `messages` end with the instruction a run sent with Draw gives
/// its closing request.
fn awaits_closing_words(messages: &[Value]) -> bool {
    messages.last().is_some_and(|last| {
        role(last) == "user" && (says(last, DRAW_THEN_SAY) || says(last, DRAW_THEN_SAY_FAILED))
    })
}

/// The index of the last `tool` message that says a picture was drawn, or
/// `None` when the history holds none.
fn drawn_picture(messages: &[Value]) -> Option<usize> {
    // Each call id the history has used so far, with whether its latest use
    // called the drawing tool: an id is reused from one turn to the next.
    let mut calls: Vec<(&str, bool)> = Vec::new();
    let mut drawn_at = None;
    for (index, message) in messages.iter().enumerate() {
        match role(message) {
            "assistant" => {
                let made = message.get("tool_calls").and_then(Value::as_array);
                for call in made.into_iter().flatten() {
                    let Some(id) = call.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    let name = call.pointer("/function/name").and_then(Value::as_str);
                    calls.retain(|(used, _)| *used != id);
                    calls.push((id, name == Some(DRAW_TOOL_NAME)));
                }
            }
            "tool" => {
                let id = message.get("tool_call_id").and_then(Value::as_str);
                let draws = calls
                    .iter()
                    .any(|(used, draws)| Some(*used) == id && *draws);
                if draws && drew(message) {
                    drawn_at = Some(index);
                }
            }
            _ => {}
        }
    }
    drawn_at
}

pub(super) fn role(message: &Value) -> &str {
    message.get("role").and_then(Value::as_str).unwrap_or("")
}

/// Each piece of text `message` carries, in either content shape.
fn texts(message: &Value) -> impl Iterator<Item = &str> {
    let content = message.get("content");
    let whole = content.and_then(Value::as_str);
    let parts = content.and_then(Value::as_array).into_iter().flatten();
    whole
        .into_iter()
        .chain(parts.filter_map(|part| part.get("text").and_then(Value::as_str)))
}

/// Whether some piece of `message`'s text holds `words`.
pub(super) fn says(message: &Value, words: &str) -> bool {
    texts(message).any(|text| text.contains(words))
}

/// Whether a `tool` message that answers the drawing tool says it drew a
/// picture the person can see.
fn drew(message: &Value) -> bool {
    texts(message).any(|text| text.starts_with(DREW_HEAD) && text.contains(DREW_SEEN))
}

fn at_front(messages: &mut Vec<Value>) {
    let appended = messages
        .first_mut()
        .filter(|first| role(first) == "system")
        .and_then(|first| first.get_mut("content"))
        .is_some_and(|content| append_text(content, DRAWN_NOTE));
    if !appended {
        messages.insert(0, json!({ "role": "system", "content": DRAWN_NOTE }));
    }
}

fn on_next_user_message(messages: &mut [Value], drawn_at: usize) -> bool {
    let mut later = messages.iter_mut().skip(drawn_at + 1);
    let Some(content) = later
        .find(|message| role(message) == "user")
        .and_then(|message| message.get_mut("content"))
    else {
        return false;
    };
    match content {
        Value::String(text) => {
            *text = if text.is_empty() {
                DRAWN_NOTE.to_owned()
            } else {
                format!("{DRAWN_NOTE}\n\n{text}")
            };
            true
        }
        Value::Array(parts) => {
            parts.insert(0, json!({ "type": "text", "text": DRAWN_NOTE }));
            true
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "drawn_note_tests.rs"]
mod drawn_note_tests;
