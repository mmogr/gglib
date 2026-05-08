//! Identity-passthrough parser for models that already speak strict `OpenAI`.
//!
//! The vast majority of models we run today emit clean
//! `LlmStreamEvent::ToolCallDelta` events directly from llama-server's
//! `tool_calls` field.  For those models the registry returns a
//! [`StandardJsonParser`], which forwards every byte unchanged and
//! never synthesises tool calls or errors.
//!
//! This parser is also the default fallback when no `format:*` tag matches.

use super::super::parser::{ParserOutput, ToolCallParser};

/// Identity-passthrough parser.  See module docs.
#[derive(Default, Debug)]
pub struct StandardJsonParser;

impl StandardJsonParser {
    /// Construct a fresh parser.  No state, so this is just `Default::default`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ToolCallParser for StandardJsonParser {
    fn push_text(&mut self, chunk: &str) -> ParserOutput {
        ParserOutput::text(chunk)
    }

    fn push_reasoning(&mut self, chunk: &str) -> ParserOutput {
        ParserOutput::reasoning(chunk)
    }

    fn finish(&mut self) -> ParserOutput {
        ParserOutput::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_passthrough_is_byte_identical() {
        let mut p = StandardJsonParser::new();
        let out = p.push_text("hello world");
        assert_eq!(out.forward_text, "hello world");
        assert!(out.forward_reasoning.is_empty());
        assert!(out.tool_calls.is_empty());
        assert!(out.errors.is_empty());
    }

    #[test]
    fn reasoning_passthrough_is_byte_identical() {
        let mut p = StandardJsonParser::new();
        let out = p.push_reasoning("thinking…");
        assert_eq!(out.forward_reasoning, "thinking…");
        assert!(out.forward_text.is_empty());
    }

    #[test]
    fn finish_emits_nothing() {
        let mut p = StandardJsonParser::new();
        let _ = p.push_text("abc");
        let out = p.finish();
        assert!(out.is_empty());
    }

    #[test]
    fn many_chunks_preserve_total_bytes() {
        let mut p = StandardJsonParser::new();
        let mut acc = String::new();
        for c in ["foo", "<tool_call>", "{\"name\":\"x\"}", "</tool_call>", "bar"] {
            acc.push_str(&p.push_text(c).forward_text);
        }
        // StandardJsonParser is identity, so even XML-looking input is
        // preserved verbatim.  Dispatch into the Qwen parser is the
        // registry's job, not this parser's.
        assert_eq!(acc, "foo<tool_call>{\"name\":\"x\"}</tool_call>bar");
    }
}
