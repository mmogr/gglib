//! What [`collect_stream`] reads after a reply's `Done`: the trailer, and
//! nothing that would make the proxy's own notice part of the reply.

use std::pin::Pin;

use anyhow::Result;
use futures_util::stream;
use gglib_agent::collect_stream;
use gglib_core::LlmStreamEvent;
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
