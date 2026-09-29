//! How long the model spent writing, timed on the upstream's own stream.
//!
//! A dialect parser holds tool-call markup back and releases it as one
//! delta at the close tag, so the normalized stream's first delta can come
//! a single decode step before its end. Timed there, a turn that only
//! calls a tool writes in no time at all and its rate is impossible. So
//! the time is taken here, on the decoded stream before normalization: from
//! the first generated token's arrival to the last. It is sent at the end
//! as [`LlmStreamEvent::WritingTime`], and only when two or more tokens
//! arrived; a time that cannot be measured is left out, never guessed.

use std::time::Instant;

use futures_util::StreamExt as _;
use gglib_core::domain::agent::LlmStreamEvent;

use super::stream::EventStream;

/// A clock in milliseconds; injected so a test can pace a stream.
pub(super) type Clock = Box<dyn Fn() -> u64 + Send>;

/// Milliseconds since this clock was made.
pub(super) fn monotonic() -> Clock {
    let start = Instant::now();
    Box::new(move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX))
}

/// Whether `event` is the model writing: a token of text, of reasoning, or
/// of a tool call.
fn is_written(event: &LlmStreamEvent) -> bool {
    match event {
        LlmStreamEvent::TextDelta { content } | LlmStreamEvent::ReasoningDelta { content } => {
            !content.is_empty()
        }
        LlmStreamEvent::ToolCallDelta { .. } => true,
        _ => false,
    }
}

/// `raw`, then its writing time once it ends.
pub(super) fn time_writing(raw: EventStream, clock: Clock) -> EventStream {
    Box::pin(async_stream::stream! {
        let mut raw = std::pin::pin!(raw);
        let mut span: Option<(u64, u64)> = None;
        while let Some(item) = raw.next().await {
            if matches!(&item, Ok(event) if is_written(event)) {
                let now = clock();
                span = Some(span.map_or((now, now), |(first, _)| (first, now)));
            }
            yield item;
        }
        if let Some((first, last)) = span.filter(|(first, last)| last > first) {
            yield Ok(LlmStreamEvent::WritingTime { ms: last - first });
        }
    })
}

#[cfg(test)]
#[path = "writing_time_tests.rs"]
mod tests;
