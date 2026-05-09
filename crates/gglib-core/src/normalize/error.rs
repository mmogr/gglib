//! Non-fatal error reporting from normalization parsers.
//!
//! Parsers in [`super::parsers`] never `Result`-fail at the trait level —
//! a malformed dialect fragment is data, not an infrastructure problem.
//! Instead, parsers attach a [`NormalizationError`] to their
//! [`super::parser::ParserOutput`], and the surrounding stream wrapper
//! surfaces those errors as
//! `LlmStreamEvent::NormalizationError` events.
//!
//! Consumers are free to log, surface, or suppress these events.  The proxy
//! drops them on the wire (V1 contract); in-process consumers (Axum/Tauri)
//! receive them for diagnostics.

/// Discriminates the kind of malformation a parser detected.
///
/// Each variant carries enough context that a developer reading a log line
/// can identify the offending bytes without rerunning the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizationErrorKind {
    /// Found a complete `<tool_call>...</tool_call>` block but its body did
    /// not parse as a JSON object with at least a `name` field.
    ///
    /// `raw` holds the body bytes between the open and close tags.
    MalformedToolCallJson { raw: String },

    /// The stream ended while we were still inside an open `<tool_call>`
    /// tag.  `partial` is the JSON body collected so far (which may be
    /// empty if only the open tag was seen).
    UnclosedToolCallTag { partial: String },

    /// A dialect-specific marker was recognised but its surrounding shape
    /// did not match any known schema.  Reserved for future parsers.
    UnknownMarkup { raw: String },
}

/// A non-fatal normalization issue surfaced from a parser.
///
/// `kind` carries the structured failure details; `raw` is a short snippet
/// of the offending input suitable for log output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizationError {
    /// Structured detail about what went wrong.
    pub kind: NormalizationErrorKind,
    /// A short, human-readable excerpt of the offending input.  Parsers
    /// should keep this small (≲ 256 bytes) so it is safe to attach to a
    /// stream event.
    pub raw: String,
}

impl NormalizationError {
    /// Construct a `MalformedToolCallJson` error with the body as `raw`.
    #[must_use]
    pub fn malformed_tool_call(body: impl Into<String>) -> Self {
        let raw = body.into();
        Self {
            kind: NormalizationErrorKind::MalformedToolCallJson { raw: raw.clone() },
            raw,
        }
    }

    /// Construct an `UnclosedToolCallTag` error from a partial body.
    #[must_use]
    pub fn unclosed_tool_call(partial: impl Into<String>) -> Self {
        let partial = partial.into();
        Self {
            kind: NormalizationErrorKind::UnclosedToolCallTag {
                partial: partial.clone(),
            },
            raw: partial,
        }
    }
}
