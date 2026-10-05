//! The turn's usage: sent once, at the stream's end, with only what the
//! upstream reported and the writing time its stream carried.

use super::*;

fn stream(events: Vec<LlmStreamEvent>) -> LlmStream {
    Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
}

async fn usage_of(events: Vec<LlmStreamEvent>) -> (usize, TurnUsage) {
    usage_after(events, 0).await
}

/// The usage of a turn whose request had `trimmed` earlier messages missing.
async fn usage_after(events: Vec<LlmStreamEvent>, trimmed: usize) -> (usize, TurnUsage) {
    let (tx, mut rx) = mpsc::channel(8);
    let passed = measure_turn(stream(events), tx, trimmed).count().await;
    let mut sent = Vec::new();
    while let Ok(event) = rx.try_recv() {
        sent.push(event);
    }
    let [AgentEvent::TurnUsage(usage)] = sent.as_slice() else {
        panic!("one turn_usage, got {sent:?}");
    };
    (passed, usage.clone())
}

fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.to_owned(),
    }
}

fn done() -> LlmStreamEvent {
    LlmStreamEvent::Done {
        finish_reason: Some("stop".to_owned()),
    }
}

fn usage(cached_tokens: Option<u32>) -> LlmStreamEvent {
    LlmStreamEvent::Usage {
        prompt_tokens: 30,
        completion_tokens: 2,
        total_tokens: 32,
        cached_tokens,
    }
}

#[tokio::test]
async fn a_turn_with_usage_sends_its_counts_and_its_streams_writing_time() {
    let (passed, usage) = usage_of(vec![
        text("hi"),
        done(),
        usage(Some(20)),
        LlmStreamEvent::WritingTime { ms: 920 },
    ])
    .await;
    assert_eq!(passed, 4, "every event passes through");
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.cached_tokens,
            usage.completion_tokens
        ),
        (Some(30), Some(20), Some(2))
    );
    assert_eq!(
        usage.writing_ms,
        Some(920),
        "the stream's own time, not one taken here"
    );
    assert_eq!(
        (usage.model.as_deref(), usage.quantization.as_deref()),
        (None, None)
    );
}

/// Absent is not zero: an upstream that reports no cached count has none.
#[tokio::test]
async fn an_unreported_cached_count_stays_absent() {
    let (_, usage) = usage_of(vec![done(), usage(None)]).await;
    assert_eq!(usage.cached_tokens, None);
    assert_eq!(usage.prompt_tokens, Some(30));
}

/// Text arrived, but nothing timed it: no writing time, so no rate.
#[tokio::test]
async fn a_turn_without_usage_or_writing_time_sends_neither() {
    let (_, usage) = usage_of(vec![text("hi"), text(" there"), done()]).await;
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.cached_tokens,
            usage.completion_tokens
        ),
        (None, None, None)
    );
    assert_eq!(usage.writing_ms, None);
}

#[tokio::test]
async fn a_stream_not_read_to_its_end_sends_nothing() {
    let (tx, mut rx) = mpsc::channel(8);
    let mut measured = measure_turn(stream(vec![done(), done()]), tx, 0);
    let _ = measured.next().await;
    drop(measured);
    assert!(rx.try_recv().is_err());
}

/// The turn says how many earlier messages its request was missing, and
/// leaves the context's size to whoever knows the model.
#[tokio::test]
async fn a_turn_after_a_prune_says_how_many_were_left_out() {
    let (_, usage) = usage_after(vec![text("hi"), done()], 4).await;
    assert_eq!(usage.reading, ContextReading::new(None, 4));
    assert_eq!(usage.reading.trimmed_messages, Some(4));
    assert_eq!(usage.reading.context_size, None);
}

/// Nothing left out is no count: the event has no `trimmed_messages` key.
#[tokio::test]
async fn a_turn_with_nothing_left_out_says_nothing_of_it() {
    let (_, usage) = usage_of(vec![text("hi"), done()]).await;
    assert_eq!(usage.reading, ContextReading::default());
    let frame = serde_json::to_value(AgentEvent::TurnUsage(usage)).unwrap();
    assert!(frame.get("trimmed_messages").is_none(), "{frame}");
    assert!(frame.get("context_size").is_none(), "{frame}");
}

/// The reason the stream's `Done` gave is the turn's, as it was spelt; a
/// stream that gave none leaves it out.
#[tokio::test]
async fn a_turn_says_why_the_model_stopped() {
    let cut_off = LlmStreamEvent::Done {
        finish_reason: Some("length".to_owned()),
    };
    let (_, usage) = usage_of(vec![text("half an ans"), cut_off, self::usage(None)]).await;
    assert_eq!(usage.finish_reason.as_deref(), Some("length"));

    let unsaid = LlmStreamEvent::Done {
        finish_reason: None,
    };
    let (_, usage) = usage_of(vec![text("hi"), unsaid]).await;
    assert_eq!(usage.finish_reason, None);
    let (_, usage) = usage_of(vec![text("hi")]).await;
    assert_eq!(usage.finish_reason, None);
}
