//! Error types for the structured-output subsystem.
//!
//! The implementation of `get_structured` lives in `gglib-agent`, where
//! `futures-util` and stream-collection helpers are already available.
//! This module holds only the shared error type so that callers in any crate
//! can pattern-match on failure modes without depending on `gglib-agent`.
//!
//! # Variants
//!
//! | Variant | Meaning |
//! |---------|---------|
//! | [`StructuredOutputError::Stream`] | The LLM stream itself failed (network, timeout, etc.) |
//! | [`StructuredOutputError::Parse`] | The collected text was not valid JSON or did not match the expected type |
//! | [`StructuredOutputError::MaxRetriesExceeded`] | All retry attempts were exhausted without a successful parse |

use thiserror::Error;

/// Failure modes for a structured-output LLM call.
///
/// Produced by `gglib_agent::structured_output::get_structured` when the
/// LLM cannot produce a valid JSON response within the allowed retry budget.
#[derive(Debug, Error)]
pub enum StructuredOutputError {
    /// The underlying [`super::LlmCompletionPort::chat_stream`] call failed
    /// before any response was collected.
    #[error("LLM stream error: {0}")]
    Stream(#[from] anyhow::Error),

    /// The LLM produced output but it could not be parsed as valid JSON (or
    /// as the expected type).  Includes the raw text and the number of
    /// attempts made so far.
    #[error("JSON parse error after {attempts} attempt(s): {error}\nRaw output: {raw}")]
    Parse {
        /// The `serde_json` error message.
        error: String,
        /// The raw text that failed to parse.
        raw: String,
        /// How many parse attempts have been made (including this one).
        attempts: u32,
    },

    /// All `max_retries + 1` attempts were exhausted without a successful
    /// parse.  The last parse error is included for diagnostics.
    #[error("structured output failed after {max_retries} retries: {last_error}")]
    MaxRetriesExceeded {
        /// The retry limit that was set by the caller.
        max_retries: u32,
        /// The parse error from the final attempt.
        last_error: String,
    },
}
