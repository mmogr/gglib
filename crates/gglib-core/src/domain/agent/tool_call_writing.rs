//! [`ToolCallWriting`]: how far the model has got writing a tool call.

use serde::Serialize;

/// How far the model has got writing a tool call, sent while it writes one.
///
/// A call arrives in pieces that the loop keeps until the reply ends, so a
/// reply that spends minutes writing one would otherwise show nothing. The
/// first piece of a reply's calls that names its tool sends one of these,
/// and later pieces at most one a second. Serialised flat, beside
/// `"type": "tool_call_writing"`.
///
/// That holds where llama-server streams the call itself. On a model whose
/// calls come as text markup (one with a dialect spec), gglib's parser holds
/// the markup back until the call is complete and releases it as one piece,
/// so one of these comes then, at the end, with a small count, and nothing
/// comes while the markup streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolCallWriting {
    /// The tool, as the call names it (`builtin:generate_image`).
    pub tool_name: String,
    /// The tool's name for a person: the prefix dropped, title-cased
    /// (`Generate Image`).
    pub display_name: String,
    /// How many pieces of this reply's calls have streamed. llama-server
    /// streams a call about one generated token a piece, so this is close
    /// to the tokens the calls have taken so far; a dialect model's call is
    /// one piece however long its markup was.
    pub tokens: u32,
}
