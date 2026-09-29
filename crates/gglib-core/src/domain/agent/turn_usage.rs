//! [`TurnUsage`]: how one model turn of the agent loop was made.

use serde::{Deserialize, Serialize};

/// How one model turn was made, sent once its stream has ended.
///
/// Counts and times only, never text. Each count is the upstream's own,
/// and absent when it reported none: absent is not zero. The model is the
/// one the run drove, stamped by whoever composed the loop; the loop itself
/// does not know it. Nothing derived is carried: a rate is
/// `completion_tokens` over `writing_ms`, computed where it is shown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnUsage {
    /// The model's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The model's quantisation, when its catalogue entry names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// Tokens in the prompt the model read, cached ones included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    /// Of `prompt_tokens`, those served from the KV cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
    /// Tokens the model wrote: reasoning, text and tool calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    /// From the stream's start to its end, in ms.
    pub duration_ms: u64,
    /// From the first thing written to the stream's end, in ms; absent when
    /// the turn wrote nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writing_ms: Option<u64>,
}
