//! The turn's usage: sent once, at the stream's end, with only what the
//! upstream reported.

use super::*;

fn stream(events: Vec<LlmStreamEvent>) -> LlmStream {
    Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
}

async fn drain(events: Vec<LlmStreamEvent>) -> (usize, Vec<AgentEvent>) {
    let (tx, mut rx) = mpsc::channel(8);
    let passed = measure_turn(stream(events), tx).count().await;
    let mut sent = Vec::new();
    while let Ok(event) = rx.try_recv() {
        sent.push(event);
    }
    (passed, sent)
}

fn done() -> LlmStreamEvent {
    LlmStreamEvent::Done {
        finish_reason: Some("stop".to_owned()),
    }
}

#[tokio::test]
async fn a_turn_with_usage_sends_its_counts_once_at_the_end() {
    let (passed, sent) = drain(vec![
        LlmStreamEvent::TextDelta {
            content: "hi".to_owned(),
        },
        done(),
        LlmStreamEvent::Usage {
            prompt_tokens: 30,
            completion_tokens: 2,
            total_tokens: 32,
            cached_tokens: Some(20),
        },
    ])
    .await;
    assert_eq!(passed, 3, "every event passes through");
    let [AgentEvent::TurnUsage(usage)] = sent.as_slice() else {
        panic!("one turn_usage, got {sent:?}");
    };
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.cached_tokens,
            usage.completion_tokens
        ),
        (Some(30), Some(20), Some(2))
    );
    assert!(usage.writing_ms.is_some());
    assert!(usage.writing_ms <= Some(usage.duration_ms));
    assert_eq!(
        (usage.model.as_deref(), usage.quantization.as_deref()),
        (None, None)
    );
}

#[tokio::test]
async fn a_turn_without_usage_sends_no_count() {
    let (_, sent) = drain(vec![done()]).await;
    let [AgentEvent::TurnUsage(usage)] = sent.as_slice() else {
        panic!("one turn_usage, got {sent:?}");
    };
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.cached_tokens,
            usage.completion_tokens
        ),
        (None, None, None)
    );
    assert_eq!(usage.writing_ms, None, "nothing was written");
}

#[tokio::test]
async fn a_stream_not_read_to_its_end_sends_nothing() {
    let (tx, mut rx) = mpsc::channel(8);
    let mut measured = measure_turn(stream(vec![done(), done()]), tx);
    let _ = measured.next().await;
    drop(measured);
    assert!(rx.try_recv().is_err());
}
