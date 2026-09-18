//! The two shapes a message's `content` takes, read in one place.
//!
//! An `OpenAI` message's `content` is a string, or an array of parts in which
//! the text parts carry a `text` field beside parts that are not text
//! (`image_url`, …). VS Code's LLM gateway sends the array form on every
//! message. Each stage that reads or rewrites message text has to handle
//! both, and until now each walked them inline where it needed them:
//! canonicalisation rewrote each shape's text in place, and truncation
//! measured the string and skipped the array. The walk lives here so a stage
//! that handles one shape handles the other.
//!
//! Everything here works on a raw [`serde_json::Value`], never on a typed
//! message, because the callers forward the body they were given: a round
//! trip through a typed message re-serialises it, and the byte stability of
//! the forwarded prompt is what canonicalisation exists to protect.

use serde_json::Value;

/// The number of characters of text `content` carries, in either shape.
///
/// A string is its own length; an array is the sum of its parts' `text`
/// fields, with the parts that are not text counting for nothing; any other
/// shape is 0.
#[must_use]
pub fn text_len(content: &Value) -> usize {
    match content {
        Value::String(text) => text.len(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .map(str::len)
            .sum(),
        _ => 0,
    }
}

/// The number of pieces of text `content` carries: 1 for a string, one per
/// text part for an array, 0 for anything else.
#[must_use]
pub fn text_parts(content: &Value) -> usize {
    match content {
        Value::String(_) => 1,
        Value::Array(parts) => parts
            .iter()
            .filter(|part| part.get("text").is_some_and(Value::is_string))
            .count(),
        _ => 0,
    }
}

/// Append `text` to `content` as a trailing piece of text, in either shape,
/// and say whether it could be.
///
/// A string gains `text` after a blank line. An array gains one more
/// `{"type": "text", "text": …}` part at the end, beside whatever parts it
/// already has — the walks above can only rewrite text that is already there,
/// so adding a piece is its own operation rather than a use of them. Any
/// other shape — `null`, absent, a number — is **left untouched** and reports
/// `false`, which is the caller's signal that this message cannot carry the
/// text and something else must.
///
/// The blank line matters: the appended text has to read as a separate
/// paragraph to a model that is about to be handed the whole thing as one
/// string, and the shapes must agree about that, because a template is free
/// to join an array's text parts with nothing between them.
pub fn append_text(content: &mut Value, text: &str) -> bool {
    match content {
        Value::String(existing) => {
            if !existing.is_empty() {
                existing.push_str("\n\n");
            }
            existing.push_str(text);
            true
        }
        Value::Array(parts) => {
            parts.push(serde_json::json!({ "type": "text", "text": text }));
            true
        }
        _ => false,
    }
}

/// Apply `f` to every piece of text in `content`, in either shape, and say
/// how many pieces it visited.
///
/// The shape is kept: a string stays a string, and an array keeps its parts
/// in order with the ones that are not text untouched. A `content` of any
/// other shape is left alone and reports 0.
pub fn for_each_text_mut(content: &mut Value, f: &mut dyn FnMut(&mut String)) -> usize {
    match content {
        Value::String(text) => {
            f(text);
            1
        }
        Value::Array(parts) => {
            let mut visited = 0;
            for part in parts.iter_mut() {
                if let Some(Value::String(text)) = part.get_mut("text") {
                    f(text);
                    visited += 1;
                }
            }
            visited
        }
        _ => 0,
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod content_tests;
