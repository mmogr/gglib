//! [`TurnUsage`]: how one model turn of the agent loop was made, and
//! [`ContextReading`]: how large its context was and what did not fit.

use std::time::Duration;

use serde::{Deserialize, Serialize, Serializer};

use crate::domain::figures::{rate_words, seconds_words, tokens_per_second, tokens_words};

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
/// both stamped by whoever composed the loop; the loop itself knows neither.
///
/// One figure is derived, and only on the wire: the writing rate
/// ([`Self::tokens_per_second`]), sent as `tokens_per_second` beside the
/// counts it is made from, so it can never disagree with them. Reading a
/// frame back ignores it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
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

impl TurnUsage {
    /// Tokens written per second of writing (`completion_tokens` over
    /// `writing_ms`); `None` without both, or with no time writing.
    #[must_use]
    pub fn tokens_per_second(&self) -> Option<f64> {
        tokens_per_second(self.completion_tokens, self.writing_ms)
    }

    /// The turn's figure line, the one under a reply in the GUI's thread and
    /// after it in `gglib chat`: how long it took, how fast it wrote and how
    /// much of the prompt the cache gave ("41 s · 13 t/s · 2,100 tokens
    /// reused"), each part only when the turn has it.
    #[must_use]
    pub fn figure_line(&self) -> String {
        let seconds = Duration::from_millis(self.duration_ms).as_secs_f64();
        [
            Some(seconds_words(seconds)),
            self.tokens_per_second().map(rate_words),
            self.cached_tokens
                .map(|cached| format!("{} reused", tokens_words(u64::from(cached)))),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
    }
}

/// The frame a [`TurnUsage`] is sent as: its fields, and its writing rate
/// beside them.
#[derive(Serialize)]
struct TurnUsageFrame<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantization: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cached_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completion_tokens: Option<u32>,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    writing_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tokens_per_second: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<&'a str>,
    #[serde(flatten)]
    reading: &'a ContextReading,
}

impl Serialize for TurnUsage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TurnUsageFrame {
            model: self.model.as_deref(),
            quantization: self.quantization.as_deref(),
            prompt_tokens: self.prompt_tokens,
            cached_tokens: self.cached_tokens,
            completion_tokens: self.completion_tokens,
            duration_ms: self.duration_ms,
            writing_ms: self.writing_ms,
            tokens_per_second: self.tokens_per_second(),
            device: self.device.as_deref(),
            finish_reason: self.finish_reason.as_deref(),
            reading: &self.reading,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
#[path = "turn_usage_tests.rs"]
mod tests;
