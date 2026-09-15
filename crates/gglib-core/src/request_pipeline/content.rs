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
