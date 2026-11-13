//! [`ToolCallWriting`]: how far the model has got writing a tool call.

use serde::{Deserialize, Serialize};

/// How far the model has got writing a tool call, sent while it writes one.
///
/// A call arrives in pieces that the loop keeps until the reply ends, so a
/// reply that spends minutes writing one would otherwise show nothing. The
/// first piece of a reply's calls that names its tool sends one of these,
/// and later pieces at most one a second. Serialised flat, beside
/// `"type": "tool_call_writing"`.
///
/// Only a call llama-server streams itself sends these. Tool-call markup
/// llama-server leaves in the text is text, so it sends none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallWriting {
    /// The tool, as the call names it (`builtin:generate_image`).
    pub tool_name: String,
    /// The tool's name for a person: the prefix dropped, title-cased
    /// (`Generate Image`).
    pub display_name: String,
    /// How many pieces of this reply's calls have streamed. llama-server
    /// streams a call about one generated token a piece, so this is close
    /// to the tokens the calls have taken so far.
    pub tokens: u32,
}
