//! [`TurnUsage`]: how one model turn of the agent loop was made, and
//! [`ContextReading`]: how large its context was and what did not fit.

use serde::{Deserialize, Serialize};

/// How large the context of the server that answered was, and how many
/// earlier messages were missing from the request it answered.
///
/// One type spells both names for every carrier: the proxy's streaming usage
/// frame carries them inside `usage`, and a run's `turn_usage` event carries
/// them flat ([`TurnUsage::reading`]). Each is absent when unknown or zero,
/// never `0` or `null`, and nothing is derived from them here: how full the
/// context is, is computed where it is shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextReading {
    /// The context, in tokens, that the server which answered was launched
    /// with. Absent when that is not known; never a default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_size: Option<u64>,
    /// Earlier messages missing from the request this model call answered:
    /// shortened to a placeholder by the proxy, or left out by the agent
    /// loop, where the count runs over the whole run so far. Absent when
    /// there were none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trimmed_messages: Option<u32>,
}

impl ContextReading {
    /// A reading of `context_size`, with `trimmed` messages missing; none
    /// trimmed is absent, not zero.
    #[must_use]
    pub fn new(context_size: Option<u64>, trimmed: usize) -> Self {
        Self {
            context_size,
            trimmed_messages: (trimmed > 0).then(|| u32::try_from(trimmed).unwrap_or(u32::MAX)),
        }
    }
}

/// How one model turn was made, sent once its stream has ended.
///
/// Counts and times only, never text. Each count is the upstream's own,
/// and absent when it reported none: absent is not zero. The model is the
/// one the run drove, and the device the paired one whose turn it answered,
/// both stamped by whoever composed the loop; the loop itself knows neither. Nothing derived is carried: a rate is
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
    /// From the first generated token's arrival to the last, in ms, timed on
    /// the upstream's stream before normalization; absent when it could not
    /// be (fewer than two tokens, or an adapter that does not time it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writing_ms: Option<u64>,
    /// The paired device whose turn this answered; absent for this
    /// machine's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Why the model stopped writing, as its stream said it (`stop`,
    /// `length` when it was cut off, `tool_calls`); absent when it gave none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// How large the context was and what did not fit, flat beside the
    /// counts. The loop counts what it left out; the context's size is
    /// stamped with the model, by whoever composed the loop.
    #[serde(flatten)]
    pub reading: ContextReading,
}
