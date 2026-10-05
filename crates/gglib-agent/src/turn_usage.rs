//! How long a model turn took, and what the upstream said it used.
//!
//! [`measure_turn`] wraps a turn's LLM stream and, when the stream ends,
//! sends one [`AgentEvent::TurnUsage`]: the upstream's token counts (each
//! only when it reported it), the time from the stream's start to its end,
//! the writing time the stream itself reports
//! ([`LlmStreamEvent::WritingTime`], timed before normalization; this
//! stream is after it, where held-back markup makes writing look instant),
//! why the model stopped ([`LlmStreamEvent::Done`]), and how many earlier
//! messages the loop had dropped from the request this turn answered.
//! A stream the collector stops reading early (an error, a cancelled run)
//! ends no turn, and sends nothing.

use std::pin::Pin;
use std::time::Instant;

use anyhow::Result;
use futures_core::Stream;
use futures_util::StreamExt as _;
use gglib_core::domain::agent::{ContextReading, TurnUsage};
use gglib_core::{AgentEvent, LlmStreamEvent};
use tokio::sync::mpsc;

type LlmStream = Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>;

/// What the turn's stream has shown so far.
struct Measuring {
    inner: LlmStream,
    tx: mpsc::Sender<AgentEvent>,
    started: Instant,
    usage: TurnUsage,
}

impl Measuring {
    fn observe(&mut self, event: &LlmStreamEvent) {
        match event {
            LlmStreamEvent::WritingTime { ms } => self.usage.writing_ms = Some(*ms),
            LlmStreamEvent::Done { finish_reason } => {
                self.usage.finish_reason.clone_from(finish_reason);
            }
            LlmStreamEvent::Usage {
                prompt_tokens,
                completion_tokens,
                cached_tokens,
                ..
            } => {
                self.usage.prompt_tokens = Some(*prompt_tokens);
                self.usage.completion_tokens = Some(*completion_tokens);
                self.usage.cached_tokens = *cached_tokens;
            }
            _ => {}
        }
    }

    fn finish(mut self) -> (mpsc::Sender<AgentEvent>, TurnUsage) {
        let ended = Instant::now();
        self.usage.duration_ms = ms_between(self.started, ended);
        (self.tx, self.usage)
    }
}

fn ms_between(from: Instant, to: Instant) -> u64 {
    u64::try_from(to.duration_since(from).as_millis()).unwrap_or(u64::MAX)
}

/// `stream`, sending the turn's [`AgentEvent::TurnUsage`] on `tx` once it ends.
///
/// `trimmed` is how many earlier messages were missing from the request the
/// stream answers; the turn reports it, and nothing when it is zero. The
/// context's size is not known here: whoever composed the loop stamps it.
pub(crate) fn measure_turn(
    stream: LlmStream,
    tx: mpsc::Sender<AgentEvent>,
    trimmed: usize,
) -> LlmStream {
    let state = Some(Measuring {
        inner: stream,
        tx,
        started: Instant::now(),
        usage: TurnUsage {
            reading: ContextReading::new(None, trimmed),
            ..TurnUsage::default()
        },
    });
    Box::pin(futures_util::stream::unfold(state, |state| async move {
        let mut measuring = state?;
        if let Some(item) = measuring.inner.next().await {
            if let Ok(event) = &item {
                measuring.observe(event);
            }
            return Some((item, Some(measuring)));
        }
        let (tx, usage) = measuring.finish();
        // Ignore a closed channel: the client may have gone.
        let _ = tx.send(AgentEvent::TurnUsage(usage)).await;
        None
    }))
}

#[cfg(test)]
#[path = "turn_usage_tests.rs"]
mod tests;
