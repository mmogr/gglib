//! One model's compare reply, read off the wire chunk by chunk.

use std::time::Instant;

use chrono::Utc;
use tracing::warn;

use gglib_core::domain::benchmark::{BenchmarkEvent, CompareConfig, ModelCompareResult};
use gglib_core::sse::DataFrames;

use crate::benchmark::mapper::{
    extract_compare_timings, extract_finish_reason, extract_reasoning_delta, extract_text_delta,
    extract_usage,
};

/// Take `chunk` off a model's reply, and return every JSON chunk it
/// completed. `[DONE]` is not one, and a payload that is not JSON is skipped.
pub(super) fn reply_chunks(
    frames: &mut DataFrames,
    chunk: &[u8],
    model: &str,
) -> Vec<serde_json::Value> {
    frames
        .push(chunk)
        .iter()
        .filter(|data| *data != "[DONE]")
        .filter_map(|data| match serde_json::from_str(data) {
            Ok(val) => Some(val),
            Err(e) => {
                warn!("benchmark: failed to parse SSE chunk for '{model}': {e}");
                None
            }
        })
        .collect()
}

/// Where a thinking model is in its reasoning.
#[derive(Debug, Clone, Copy, Default)]
enum Thinking {
    /// No reasoning and no answer yet.
    #[default]
    NotYet,
    /// Reasoning since this instant, and no answer yet.
    Since(Instant),
    /// The answer has begun, or the stream has ended.
    Over,
}

/// One model's reply, read chunk by chunk: the answer, the running totals the
/// record keeps, and how long the model thought before answering.
///
/// Reasoning (`reasoning_content`) is never part of the answer. It shows only
/// as two [`BenchmarkEvent::ModelThinking`] events: one when it starts, one
/// with its length when the answer starts or the stream ends. The clock is the
/// caller's, so a test can hand in the instants it wants.
#[derive(Debug, Default)]
pub(super) struct ReplyFold {
    model_id: i64,
    response_text: String,
    was_truncated: bool,
    /// A chunk carried a `finish_reason`: the reply ended rather than broke off.
    finished: bool,
    prompt_ms: Option<f64>,
    generation_ms: Option<f64>,
    prompt_tps: Option<f64>,
    generation_tps: Option<f64>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    thinking: Thinking,
}

impl ReplyFold {
    pub(super) fn new(model_id: i64) -> Self {
        Self {
            model_id,
            ..Self::default()
        }
    }

    /// Read one JSON chunk that arrived at `now`, and return the events it
    /// gives rise to, in the order they are to be sent.
    pub(super) fn absorb(&mut self, val: &serde_json::Value, now: Instant) -> Vec<BenchmarkEvent> {
        let mut events = Vec::new();
        if matches!(self.thinking, Thinking::NotYet) && extract_reasoning_delta(val).is_some() {
            self.thinking = Thinking::Since(now);
            events.push(BenchmarkEvent::ModelThinking {
                model_id: self.model_id,
                seconds: None,
            });
        }
        if let Some(text) = extract_text_delta(val) {
            // Before the text, so a terminal can overwrite its `thinking…`
            // line before the answer prints under it.
            events.extend(self.stop_thinking(now));
            self.response_text.push_str(&text);
            events.push(BenchmarkEvent::ModelTextDelta {
                model_id: self.model_id,
                text,
            });
        }
        if let Some(reason) = extract_finish_reason(val) {
            self.finished = true;
            self.was_truncated |= reason == "length";
        }
        // Each total is updated only by a chunk that carries it.
        let (pm, gm, pt, gt) = extract_compare_timings(val);
        self.prompt_ms = pm.or(self.prompt_ms);
        self.generation_ms = gm.or(self.generation_ms);
        self.prompt_tps = pt.or(self.prompt_tps);
        self.generation_tps = gt.or(self.generation_tps);
        let (ptu, ctu) = extract_usage(val);
        self.prompt_tokens = ptu.or(self.prompt_tokens);
        self.completion_tokens = ctu.or(self.completion_tokens);
        events
    }

    /// The stream ended at `now`: the length of a reasoning no answer followed.
    pub(super) fn finish(&mut self, now: Instant) -> Option<BenchmarkEvent> {
        self.stop_thinking(now)
    }

    /// End the thinking phase, and return its length if one was running.
    fn stop_thinking(&mut self, now: Instant) -> Option<BenchmarkEvent> {
        let since = match std::mem::replace(&mut self.thinking, Thinking::Over) {
            Thinking::Since(since) => since,
            Thinking::NotYet | Thinking::Over => return None,
        };
        Some(BenchmarkEvent::ModelThinking {
            model_id: self.model_id,
            seconds: Some(now.saturating_duration_since(since).as_secs_f64()),
        })
    }

    pub(super) fn into_result(self, config: &CompareConfig, run_id: i64) -> ModelCompareResult {
        // A stream that broke off before any `finish_reason` was cut short.
        // An empty answer that did finish is not: a thinking model can spend
        // its reply on reasoning, which is not part of the answer.
        let was_truncated = self.was_truncated || !self.finished;
        ModelCompareResult {
            id: None,
            model_id: self.model_id,
            run_id: Some(run_id),
            prompt_text: config.prompt.clone(),
            system_prompt: config.system_prompt.clone(),
            response_text: self.response_text,
            was_truncated,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            prompt_ms: self.prompt_ms,
            generation_ms: self.generation_ms,
            prompt_tps: self.prompt_tps,
            generation_tps: self.generation_tps,
            created_at: Utc::now(),
        }
    }
}
