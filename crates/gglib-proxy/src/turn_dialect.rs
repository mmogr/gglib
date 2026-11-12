//! The dialect a turn's answer is read in.
//!
//! Both of the proxy's chat paths read an answer twice over: the dialect
//! parser turns its markup into `tool_calls`, and the drift alarm scans what
//! is left for markup that survived.

use gglib_core::domain::DialectSpec;

/// A model's dialect for one turn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TurnDialect {
    spec: Option<DialectSpec>,
}

impl TurnDialect {
    /// `spec`, read by the parser and scanned.
    pub(crate) const fn parsed(spec: Option<DialectSpec>) -> Self {
        Self { spec }
    }

    /// The spec the parser is handed: `None` selects the passthrough.
    pub(crate) const fn parser_spec(&self) -> Option<&DialectSpec> {
        self.spec.as_ref()
    }

    /// The spec the drift alarm scans for.
    pub(crate) const fn scanned_spec(&self) -> Option<&DialectSpec> {
        self.spec.as_ref()
    }
}
