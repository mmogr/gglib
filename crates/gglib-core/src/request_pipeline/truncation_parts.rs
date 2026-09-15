//! The elision of one message, in either content shape.
//!
//! Split out of [`super::truncation`], which is at its file budget, and kept
//! beside it because the rule is the same for both shapes: when a message's
//! text is over the threshold, every piece of it becomes the placeholder and
//! the shape stays. A multi-part message keeps the parts that are not text
//! (an `image_url` beside the tool output) where they were, and stays
//! readable by the client that sent it in that shape.
//!
//! Until this existed only the string form was elided and the array form was
//! skipped, so a long array-form history was refused as over budget while
//! every one of its messages was eligible. VS Code's LLM gateway sends the
//! array form on every message.

use serde_json::Value;

use super::content::{for_each_text_mut, text_len, text_parts};
use super::truncation::{TOOL_CONTENT_THRESHOLD_CHARS, TRUNCATION_PLACEHOLDER};

/// Replace this message's text with the placeholder when it exceeds
/// [`TOOL_CONTENT_THRESHOLD_CHARS`], and say how many characters that
/// reclaimed.
///
/// `None` when nothing was changed: the message is under the threshold,
/// carries no text, or is split into so many short text parts that one
/// placeholder per part would not be smaller than the text it replaces. Each
/// piece of text becomes the placeholder, so a message of several text parts
/// carries the placeholder once per part, and the estimate counts it once per
/// part.
pub(super) fn elide(msg: &mut Value) -> Option<usize> {
    let content = msg.get_mut("content")?;
    let text_chars = text_len(content);
    let placeholders = text_parts(content) * TRUNCATION_PLACEHOLDER.len();
    if text_chars <= TOOL_CONTENT_THRESHOLD_CHARS || text_chars <= placeholders {
        return None;
    }
    for_each_text_mut(content, &mut |text| {
        TRUNCATION_PLACEHOLDER.clone_into(text);
    });
    Some(text_chars - placeholders)
}

#[cfg(test)]
#[path = "truncation_parts_tests.rs"]
mod truncation_parts_tests;
