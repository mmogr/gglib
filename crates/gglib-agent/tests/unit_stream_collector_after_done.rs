//! What [`collect_stream`] reads after a reply's `Done`: the trailer, and
//! nothing that would make the proxy's own notice part of the reply. The
//! notice is no part of the reply before a `Done` either, which is where it
//! arrives when the model's stream ended with no finish frame.

use std::pin::Pin;

use anyhow::Result;
use futures_util::stream;
use gglib_agent::collect_stream;
use gglib_core::{AgentEvent, LlmStreamEvent};
use tokio::sync::mpsc;

fn make_stream(
    events: Vec<LlmStreamEvent>,
) -> Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>> {
    Box::pin(stream::iter(events.into_iter().map(Ok)))
}

/// What a reply through the proxy reads as when the model only reasoned: the
/// finish frame, the usage frame, then the proxy's own notice as text, sent
/// after the finish and before `[DONE]`. The reply is what came before its
/// `Done`; the notice is the proxy's, and the usage still counts.
#[tokio::test]
async fn text_after_done_is_not_the_reply() {
    let (tx, _rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::ReasoningDelta {
            content: "The answer is 42.".into(),
        },
        LlmStreamEvent::Done {
            finish_reason: Some("stop".into()),
        },
        LlmStreamEvent::Usage {
            prompt_tokens: 10,
            completion_tokens: 6,
            total_tokens: 16,
            cached_tokens: None,
        },
        LlmStreamEvent::TextDelta {
            content: "\n\n⚠️ [proxy: reasoning-only response] The answer is 42.".into(),
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    assert_eq!(response.content, "");
    assert_eq!(response.reasoning_content, "The answer is 42.");
    assert_eq!(response.completion_tokens, Some(6));
}

/// The text events `rx` holds, in order.
fn sent_as_text(mut rx: mpsc::Receiver<AgentEvent>) -> Vec<String> {
    let mut sent = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let AgentEvent::TextDelta { content } = event {
            sent.push(content);
        }
    }
    sent
}

/// What a reply through the proxy reads as when the model only reasoned and
/// its stream ended with no finish frame: the proxy's notice as text, then
/// the `Done` the decoder gives a stream that had none. The notice is the
/// proxy's, not the reply, and is not sent on as the model's text.
#[tokio::test]
async fn a_reasoning_only_notice_with_no_finish_frame_before_it_is_not_the_reply() {
    let (tx, rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::ReasoningDelta {
            content: "The answer is 42.".into(),
        },
        LlmStreamEvent::TextDelta {
            content: "\n\n⚠️ [proxy: reasoning-only response] The answer is 42.".into(),
        },
        LlmStreamEvent::Done {
            finish_reason: None,
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    drop(tx);

    assert_eq!(response.content, "");
    assert_eq!(response.reasoning_content, "The answer is 42.");
    assert_eq!(sent_as_text(rx), Vec::<String>::new());
}

/// The same for a model that wrote nothing at all: the proxy's notice that
/// it produced no output, with the finish reason it adds, is not the reply.
#[tokio::test]
async fn a_no_output_notice_with_no_finish_frame_before_it_is_not_the_reply() {
    let (tx, rx) = mpsc::channel(16);
    let notice = "⚠️ [proxy] The model produced no output for this request. The upstream server \
                  may be overloaded or degraded — retry, and if it persists restart the model. \
                  (finish_reason: none)";
    let stream = make_stream(vec![
        LlmStreamEvent::TextDelta {
            content: notice.into(),
        },
        LlmStreamEvent::Done {
            finish_reason: None,
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    drop(tx);

    assert_eq!(response.content, "");
    assert_eq!(sent_as_text(rx), Vec::<String>::new());
}

/// The model's own text before its `Done` is the reply, as it always was.
#[tokio::test]
async fn the_models_text_with_no_finish_frame_is_the_reply() {
    let (tx, rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::TextDelta {
            content: "It is in src/lib.rs.".into(),
        },
        LlmStreamEvent::Done {
            finish_reason: None,
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    drop(tx);

    assert_eq!(response.content, "It is in src/lib.rs.");
    assert_eq!(sent_as_text(rx), ["It is in src/lib.rs."]);
}
