//! Stray `<think>` / `</think>` boundary tags in a reply's text, removed by
//! the one function both paths call: [`super::stream::NormalizingStream`]
//! on each text delta, and
//! [`super::oneshot::normalize_chat_completion_body`] on a whole reply.

/// `text` without its `<think>` and `</think>` tags; what is between them
/// stays.
///
/// A reasoning model (Qwen3, for one) sends its chain of thought in
/// `reasoning_content` but leaks the closing `</think>` into `content` as
/// it turns to its answer. The tag means nothing to a client and shows up
/// verbatim in its chat pane.
pub(crate) fn strip_think_tags(text: &str) -> String {
    text.replace("</think>", "").replace("<think>", "")
}

#[cfg(test)]
#[path = "think_tags_tests.rs"]
mod think_tags_tests;
