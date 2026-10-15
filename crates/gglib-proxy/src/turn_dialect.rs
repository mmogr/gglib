//! The dialect a turn's answer is read in, and whether the parser reads it.
//!
//! Both of the proxy's chat paths read an answer twice over: the dialect
//! parser turns its markup into `tool_calls`, and the drift alarm scans what
//! is left for markup that survived. Under
//! [`DISABLE_DIALECT_PARSER_ENV`](gglib_core::request_pipeline::DISABLE_DIALECT_PARSER_ENV)
//! the two part: the parser is handed no spec and passes the text through,
//! while the scan keeps the spec and counts every leak. That is what lets a
//! release cycle measure the parser's deletion criterion.

use std::borrow::Cow;

use gglib_core::domain::DialectSpec;
use gglib_core::request_pipeline::{ModelContext, dialect_parser_disabled};

/// A model's dialect for one turn, and whether the parser reads it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TurnDialect {
    spec: Option<DialectSpec>,
    parsed: bool,
}

impl TurnDialect {
    /// `spec`, read by the parser and scanned.
    pub(crate) const fn parsed(spec: Option<DialectSpec>) -> Self {
        Self { spec, parsed: true }
    }

    /// `spec`, scanned but not parsed.
    pub(crate) const fn bypassed(spec: Option<DialectSpec>) -> Self {
        Self {
            spec,
            parsed: false,
        }
    }

    /// `spec` as the switch in this process says to read it now.
    pub(crate) fn for_turn(spec: Option<DialectSpec>) -> Self {
        if dialect_parser_disabled() {
            Self::bypassed(spec)
        } else {
            Self::parsed(spec)
        }
    }

    /// The spec the parser is handed: `None` selects the passthrough.
    pub(crate) fn parser_spec(&self) -> Option<&DialectSpec> {
        self.spec.as_ref().filter(|_| self.parsed)
    }

    /// The spec the drift alarm scans for, parsed or not.
    pub(crate) const fn scanned_spec(&self) -> Option<&DialectSpec> {
        self.spec.as_ref()
    }

    /// The context the request pipeline shapes this turn's request with.
    ///
    /// Bypassed, it is `context` with no dialect, which turns off stage 6:
    /// its grammar rewrites `tool_choice` to `"none"` and needs the parser to
    /// turn the markup back into `tool_calls`. Stage 6 is the only stage that
    /// reads the dialect, so nothing else in the request changes.
    pub(crate) fn shaping_context<'a>(&self, context: &'a ModelContext) -> Cow<'a, ModelContext> {
        if self.parsed {
            Cow::Borrowed(context)
        } else {
            Cow::Owned(ModelContext {
                dialect: None,
                ..context.clone()
            })
        }
    }
}

#[cfg(test)]
#[path = "turn_dialect_tests.rs"]
mod turn_dialect_tests;
