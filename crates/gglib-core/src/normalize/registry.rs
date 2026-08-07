//! Dialect-spec parser dispatch.
//!
//! This module is the **single source of truth** for dialect selection, in
//! two layers:
//!
//! * [`dialect_for_tags`] maps legacy `format:*` tags to a built-in
//!   [`DialectSpec`] — the back-compat path for catalog rows persisted
//!   before specs existed (and for models whose spec could not be derived).
//! * [`get_parser`] turns a resolved spec into a parser.  Any spec drives
//!   the delimited parser; no spec means the identity passthrough.
//!
//! Adding a new *builtin* dialect is one entry in [`dialect_for_tags`] (plus
//! its constant in [`super::tags`]).  Template-derived dialects need no code
//! at all: detection persists a spec and it arrives here as `Some`.
//!
//! No other crate looks at `format:*` tags for parser selection — they
//! resolve a spec (usually via `ModelContext`) and call `get_parser`.  This
//! keeps the dialect surface area tightly contained and prevents drift
//! between callers.

use super::parser::ToolCallParser;
use super::parsers::{delimited::DelimitedToolCallParser, standard::StandardJsonParser};
use super::tags;
use crate::domain::dialect::DialectSpec;

/// Map legacy `format:*` tags to a built-in [`DialectSpec`].
///
/// Tags are scanned in the listed order and the first recognised tag wins.
/// Returns `None` for models with no recognised tag — the common case.
///
/// Both [`tags::FORMAT_QWEN_XML`] and [`tags::FORMAT_HERMES`] map to the
/// built-in Qwen spec: the envelope-plus-JSON dialect is shared, and the
/// spec's inner-XML fallback codec is Hermes-style to begin with.
#[must_use]
pub fn dialect_for_tags(model_tags: &[String]) -> Option<DialectSpec> {
    for t in model_tags {
        // Future builtin dialects slot in here, one arm each.
        match t.as_str() {
            tags::FORMAT_QWEN_XML | tags::FORMAT_HERMES => return Some(DialectSpec::qwen_xml()),
            _ => {}
        }
    }
    None
}

/// Pick a parser for a resolved dialect.
///
/// `Some(spec)` — from the model's persisted spec or the
/// [`dialect_for_tags`] fallback — yields a
/// [`DelimitedToolCallParser`] configured with it; `None` yields the
/// identity-passthrough [`StandardJsonParser`].
///
/// The returned trait object is `Send` because [`ToolCallParser`] requires
/// `Send`; this lets `NormalizingStream` live on a tokio task without
/// adding a separate bound.
#[must_use]
pub fn get_parser(dialect: Option<&DialectSpec>) -> Box<dyn ToolCallParser> {
    match dialect {
        Some(spec) => Box::new(DelimitedToolCallParser::new(spec.clone())),
        None => Box::new(StandardJsonParser::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_dialect_yields_standard_parser() {
        let mut p = get_parser(None);
        let out = p.push_text("hello");
        assert_eq!(out.forward_text, "hello");
    }

    #[test]
    fn qwen_tag_maps_to_the_builtin_spec_and_parses() {
        let dialect = dialect_for_tags(&[tags::FORMAT_QWEN_XML.to_owned()]);
        assert_eq!(dialect, Some(DialectSpec::qwen_xml()));

        let mut p = get_parser(dialect.as_ref());
        let out = p.push_text(r#"<tool_call>{"name":"x","arguments":{}}</tool_call>"#);
        let f = p.finish();
        assert_eq!(out.tool_calls.len(), 1);
        assert!(
            f.tool_calls.is_empty(),
            "tool calls flush in push, not finish"
        );
    }

    /// The hermes tag has been emitted by detection since it existed, but
    /// nothing consumed it — models carrying it leaked raw `<tool_call>`
    /// markup. It now maps to the same builtin spec as the qwen tag.
    #[test]
    fn hermes_tag_maps_to_the_builtin_spec() {
        let dialect = dialect_for_tags(&[tags::FORMAT_HERMES.to_owned()]);
        assert_eq!(dialect, Some(DialectSpec::qwen_xml()));
    }

    #[test]
    fn unknown_tags_yield_no_dialect() {
        assert_eq!(
            dialect_for_tags(&["format:does-not-exist".to_owned()]),
            None
        );
        assert_eq!(dialect_for_tags(&[]), None);

        let mut p = get_parser(None);
        let out = p.push_text("<tool_call>passthrough</tool_call>");
        assert_eq!(out.forward_text, "<tool_call>passthrough</tool_call>");
    }

    #[test]
    fn first_recognised_tag_wins() {
        let tags_v = vec![
            "format:does-not-exist".to_owned(),
            tags::FORMAT_QWEN_XML.to_owned(),
        ];
        let dialect = dialect_for_tags(&tags_v);
        let mut p = get_parser(dialect.as_ref());
        let out = p.push_text(r#"<tool_call>{"name":"x","arguments":{}}</tool_call>"#);
        assert_eq!(out.forward_text, "");
        assert_eq!(out.tool_calls.len(), 1);
    }

    /// An explicit spec — the template-derived path — needs no tag at all.
    #[test]
    fn an_explicit_spec_drives_the_delimited_parser() {
        let spec = DialectSpec {
            tool_open: "«TC»".to_owned(),
            tool_close: "«/TC»".to_owned(),
            ..DialectSpec::qwen_xml()
        };
        let mut p = get_parser(Some(&spec));
        let out = p.push_text(r#"«TC»{"name":"x","arguments":{}}«/TC»"#);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.forward_text, "");
    }
}
