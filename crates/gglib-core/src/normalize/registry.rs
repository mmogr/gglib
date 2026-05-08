//! Tag-driven parser dispatch.
//!
//! [`get_parser`] is the **single source of truth** for which parser handles
//! which dialect.  Adding a new parser is exactly two file touches:
//!
//! 1. Drop a new module under [`super::parsers`].
//! 2. Add **one** match arm here.
//!
//! No other crate looks at `format:*` tags — they call `get_parser` and use
//! the returned trait object.  This keeps the dialect surface area tightly
//! contained and prevents drift between callers.

use super::parser::ToolCallParser;
use super::parsers::{qwen_xml::QwenXmlParser, standard::StandardJsonParser};
use super::tags;

/// Pick a parser for a model based on its `tags` list.
///
/// Tags are scanned in the listed order and the first recognised
/// `format:*` tag wins.  Models with no recognised tag — the common case —
/// receive the identity-passthrough [`StandardJsonParser`].
///
/// The returned trait object is `Send` because [`ToolCallParser`] requires
/// `Send`; this lets `NormalizingStream` live on a tokio task without
/// adding a separate bound.
#[must_use]
pub fn get_parser(model_tags: &[String]) -> Box<dyn ToolCallParser> {
    for t in model_tags {
        // Future parsers slot in here, one arm each.  Keep this `match`
        // even with a single arm so adding a new dialect is purely
        // additive — no structural rewrite required.
        #[allow(clippy::single_match)]
        match t.as_str() {
            tags::FORMAT_QWEN_XML => return Box::new(QwenXmlParser::new()),
            _ => {}
        }
    }
    Box::new(StandardJsonParser::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tags_yield_standard_parser() {
        let mut p = get_parser(&[]);
        let out = p.push_text("hello");
        assert_eq!(out.forward_text, "hello");
    }

    #[test]
    fn qwen_tag_yields_qwen_parser() {
        let mut p = get_parser(&[tags::FORMAT_QWEN_XML.to_owned()]);
        let out = p.push_text(r#"<tool_call>{"name":"x","arguments":{}}</tool_call>"#);
        let f = p.finish();
        assert_eq!(out.tool_calls.len(), 1);
        assert!(f.tool_calls.is_empty(), "tool calls flush in push, not finish");
    }

    #[test]
    fn unknown_tag_falls_back_to_standard() {
        let mut p = get_parser(&["format:does-not-exist".to_owned()]);
        let out = p.push_text("<tool_call>passthrough</tool_call>");
        assert_eq!(out.forward_text, "<tool_call>passthrough</tool_call>");
    }

    #[test]
    fn first_recognised_tag_wins() {
        let tags_v = vec![
            "format:does-not-exist".to_owned(),
            tags::FORMAT_QWEN_XML.to_owned(),
        ];
        let mut p = get_parser(&tags_v);
        let out = p.push_text(r#"<tool_call>{"name":"x","arguments":{}}</tool_call>"#);
        assert_eq!(out.forward_text, "");
        assert_eq!(out.tool_calls.len(), 1);
    }
}
