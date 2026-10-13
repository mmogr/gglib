//! What one turn generated, as the proxy read it: the `completion_tokens`,
//! `reasoning_chars`, `tool_calls`, `finish_reason` and `generation_ms` of each
//! `recent_requests` entry.
//!
//! A streamed turn is metered while [`crate::forward::drain_events`] runs
//! ([`Meter`]); a non-streaming one is read from the body the client is
//! answered with ([`TurnGeneration::of_unary_body`]). Either way the readings
//! are written onto the request's snapshot once the turn is over, by
//! [`ContextMetricsStore::flag_generation`](crate::metrics::ContextMetricsStore::flag_generation).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;
use tokio::time::Instant;

use crate::forward::StreamOutcome;

/// What one turn generated: the five readings a snapshot is given, under
/// the [`ContextSnapshot`](crate::metrics::ContextSnapshot) fields of the
/// same names, which say what each is.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TurnGeneration {
    /// See [`crate::metrics::ContextSnapshot::completion_tokens`].
    pub completion_tokens: Option<u32>,
    /// See [`crate::metrics::ContextSnapshot::reasoning_chars`].
    pub reasoning_chars: usize,
    /// See [`crate::metrics::ContextSnapshot::tool_calls`].
    pub tool_calls: u32,
    /// See [`crate::metrics::ContextSnapshot::finish_reason`].
    pub finish_reason: Option<String>,
    /// See [`crate::metrics::ContextSnapshot::generation_ms`].
    pub generation_ms: Option<u64>,
}

impl TurnGeneration {
    /// The readings of a streamed turn, from what its drain reported.
    pub(crate) fn of_stream(outcome: &StreamOutcome) -> Self {
        Self {
            completion_tokens: outcome.completion_tokens,
            reasoning_chars: outcome.reasoning_chars,
            tool_calls: outcome.tool_calls,
            finish_reason: outcome.finish_reason.clone(),
            generation_ms: outcome.generation_ms,
        }
    }

    /// The readings `snapshot` holds.
    #[cfg(test)]
    pub(crate) fn on(snapshot: &crate::metrics::ContextSnapshot) -> Self {
        Self {
            completion_tokens: snapshot.completion_tokens,
            reasoning_chars: snapshot.reasoning_chars,
            tool_calls: snapshot.tool_calls,
            finish_reason: snapshot.finish_reason.clone(),
            generation_ms: snapshot.generation_ms,
        }
    }

    /// The readings of a non-streaming turn, from the body the client is
    /// answered with. A body that is not JSON reads as the default.
    pub(crate) fn of_unary_body(body: &[u8]) -> Self {
        let Ok(body) = serde_json::from_slice::<Value>(body) else {
            return Self::default();
        };
        let message = body.pointer("/choices/0/message");
        Self {
            completion_tokens: body
                .pointer("/usage/completion_tokens")
                .and_then(Value::as_u64)
                .and_then(|tokens| u32::try_from(tokens).ok()),
            reasoning_chars: message
                .and_then(|m| m.get("reasoning_content"))
                .and_then(Value::as_str)
                .map_or(0, |reasoning| reasoning.chars().count()),
            tool_calls: message
                .and_then(|m| m.get("tool_calls"))
                .and_then(Value::as_array)
                .map_or(0, |calls| u32::try_from(calls.len()).unwrap_or(u32::MAX)),
            finish_reason: body
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            generation_ms: None,
        }
    }
}

/// When the upstream streamed a turn's first generated token and its latest:
/// the clock behind `generation_ms`.
///
/// [`crate::forward::drain_events`] stamps it as each event arrives from the
/// upstream, before normalization, which can hold tokens back and make one
/// event of many. An event stamps when
/// [`is_generated_token`](crate::upstream_read::is_generated_token) accepts
/// it: a text, reasoning or tool-call delta. Prefill progress, `Usage`,
/// `Done`, `WritingTime`, an upstream error frame and a failed read do not
/// stamp.
///
/// That test does not look inside a delta, so an empty one stamps like any
/// other. From a real upstream none arrives as text or reasoning, because the
/// SSE parser makes no event of an empty `content` or `reasoning_content`; a
/// tool-call fragment is an event whatever it carries, and stamps.
///
/// Reads tokio's clock, which a test can pause and advance. A clone shares
/// the one pair of stamps.
#[derive(Debug, Default, Clone)]
pub(crate) struct TokenClock(Arc<Mutex<Option<(Instant, Instant)>>>);

impl TokenClock {
    /// A generated token arrived from the upstream just now.
    pub(crate) fn stamp(&self) {
        let now = Instant::now();
        let mut span = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let first = span.map_or(now, |(first, _)| first);
        *span = Some((first, now));
    }

    /// Milliseconds between the first stamp and the latest, `None` when
    /// there was no stamp.
    fn millis(&self) -> Option<u64> {
        let span = *self.0.lock().unwrap_or_else(PoisonError::into_inner);
        span.map(|(first, last)| {
            u64::try_from(last.saturating_duration_since(first).as_millis()).unwrap_or(u64::MAX)
        })
    }
}

/// Meters one streamed turn as its drain runs: the time on its
/// [`TokenClock`], stamped before normalization, and the reasoning and calls
/// the drain tells it of, after.
#[derive(Debug, Default)]
pub(crate) struct Meter {
    clock: TokenClock,
    reasoning_chars: usize,
    /// The index of every call that had a frame queued for the client.
    calls_sent: BTreeSet<usize>,
}

impl Meter {
    /// The clock whose span this meter reports, for the drain to stamp.
    pub(crate) fn clock(&self) -> TokenClock {
        self.clock.clone()
    }

    /// A reasoning delta was read out of normalization.
    pub(crate) fn reasoning(&mut self, content: &str) {
        self.reasoning_chars += content.chars().count();
    }

    /// A frame of the call at `index` was queued for the client.
    pub(crate) fn call_sent(&mut self, index: usize) {
        self.calls_sent.insert(index);
    }

    /// Write what was metered onto `outcome`.
    pub(crate) fn report(&self, outcome: &mut StreamOutcome) {
        outcome.reasoning_chars = self.reasoning_chars;
        outcome.tool_calls = u32::try_from(self.calls_sent.len()).unwrap_or(u32::MAX);
        outcome.generation_ms = self.clock.millis();
    }
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;
