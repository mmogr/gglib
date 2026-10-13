//! Stage 1c: a request that carries tools is told to batch its tool calls.
//!
//! This stage adds one note, [`AGENTIC_NOTE`], to the system prompt of a
//! request that carries tools. The note asks the model to make independent
//! reads and searches in one turn. It is off unless a caller turns it on, and
//! it has not been measured: nothing here says the note changes how a model
//! calls tools.
//!
//! # When
//!
//! The stage runs only for a caller that set
//! [`SamplingLayers::agentic_prompt_note`](super::SamplingLayers::agentic_prompt_note);
//! the proxy sets it from the `agentic_prompt_note` setting and no other
//! caller does. The agentic-sampling switch and
//! `GGLIB_DISABLE_AGENTIC_SAMPLING` govern sampling changes and do not gate
//! it. Then all of these:
//!
//! 1. The request [carries tools](super::carries_tools): a non-empty `tools`
//!    array.
//! 2. Stage 2b will not take them: the model is not one the catalog resolved
//!    as unable to call tools ([`strips_tools`]). Stage 1b has no such case to
//!    follow; this stage is handed the model's facts and reads them.
//! 3. Neither the first `system` message nor the first message of all
//!    already holds the note. The first message is read because stage 2
//!    leaves the system prompt there, as user text, for a model with no
//!    system role.
//!
//! Rule 3 does not cover every body run through [`apply`](super::apply())
//! twice. For a model with no system role, stage 2 turns a first `system`
//! message that is not the first message into user text, and unless a
//! strict-turns merge carries that text into the first message, the second
//! run finds the note in neither place and adds another. The proxy shapes
//! each request once, from the bytes the client sent, and does not meet that
//! case.
//!
//! # Where
//!
//! Appended to the first `system` message, after a blank line, or as one
//! more text part where that message's content is an array of parts
//! ([`append_text`]). A request with no `system` message, or whose first one
//! has no content that text can be added to, gains a `system` message of its
//! own before everything else. Every byte the client sent before the note is
//! left as it was, and the note is a constant with nothing of the request in
//! it, so the front of the prompt is the same bytes turn after turn.
//!
//! [`drawn_note`](super::drawn_note) adds its note to a first message that is
//! a `system` one. This stage looks for the first `system` message wherever
//! it is, so it shares that module's readers and not its placement.

use serde_json::{Value, json};

use super::content::append_text;
use super::drawn_note::{role, says};
use super::tools::strips_tools;
use super::{ModelContext, carries_tools};

/// What a request that carries tools is told, when the note is on.
pub const AGENTIC_NOTE: &str = "When several reads or searches are independent, make all of \
    those tool calls in one turn.\n\
    Search for a name before reading a file.\n\
    Read the part of a file you need, not the whole file.\n\
    Say what you will do in one line, then call the tools.";

/// Add [`AGENTIC_NOTE`] to `body`'s system prompt when the [module](self)'s
/// rule says so. Returns whether `body` changed.
pub(crate) fn note_batching(body: &mut Value, ctx: &ModelContext) -> bool {
    if !carries_tools(body) || strips_tools(ctx) {
        return false;
    }
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };
    let system = messages.iter().position(|m| role(m) == "system");
    let told = |at: usize| messages.get(at).is_some_and(|m| says(m, AGENTIC_NOTE));
    if told(0) || system.is_some_and(told) {
        return false;
    }
    let appended = system
        .and_then(|at| messages[at].get_mut("content"))
        .is_some_and(|content| append_text(content, AGENTIC_NOTE));
    if !appended {
        messages.insert(0, json!({ "role": "system", "content": AGENTIC_NOTE }));
    }
    true
}

#[cfg(test)]
#[path = "agentic_note_tests.rs"]
mod agentic_note_tests;
