//! What a streamed reply's usage frame also says of its context, and to
//! whom.
//!
//! The proxy knows two things about every chat completion that no client
//! can work out: the context the server that answered was launched with
//! (the model list's figure is shaved, and is the running server's only
//! for the model loaded when the list was read), and how many earlier
//! messages it shortened so the request fit. A client that set
//! `return_progress` in its own body is told both, inside the `usage`
//! object of the stream's usage frame. A chat run is such a client: it
//! sets the key in the body it sends, whatever its own caller sent. Any
//! other client is sent the frames it always was, byte for byte: a strict
//! `OpenAI` client never sees a key it did not ask for.

use gglib_core::domain::agent::ContextReading;
use gglib_core::request_pipeline::TruncationReport;

/// What one streaming client is told: its reply's reading when its own body
/// asked for progress, and `None` when it did not.
///
/// One value, because the two are one question. A client that asked is sent
/// `prompt_progress` frames and a reading; one that did not is sent a
/// comment for each of those frames and the usage frame unchanged.
pub(crate) type Told = Option<ContextReading>;

/// The reading for a request the server at `effective_ctx` answers, after
/// `report` says what shaping did to it; `None` unless the client `wanted`
/// progress.
pub(crate) fn told(wanted: bool, effective_ctx: u64, report: &TruncationReport) -> Told {
    wanted.then(|| ContextReading::new(Some(effective_ctx), report.messages_truncated))
}

#[cfg(test)]
#[path = "usage_reading_tests.rs"]
mod tests;
