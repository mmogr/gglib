//! While a reply's tool call streams, the collector reports how far it has
//! got as `tool_call_writing`: once for the first piece that names the
//! tool, then at most once a second, with every piece counted. Time is
//! paused, so only the pieces' own spacing moves the clock.

use std::pin::Pin;
use std::time::Duration;

use anyhow::Result;
use futures_util::{StreamExt as _, stream};
use gglib_agent::collect_stream;
use gglib_core::domain::agent::ToolCallWriting;
use gglib_core::{AgentEvent, LlmStreamEvent};
use tokio::sync::mpsc;

const TOOL: &str = "builtin:generate_image";

type Events = Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>>;

/// A call to the tool in `pieces` pieces, each `apart` after the one before,
/// then the end of the reply.
fn written(pieces: usize, apart: Duration) -> Events {
    let mut events = vec![LlmStreamEvent::ToolCallDelta {
        index: 0,
        id: Some("call_1".into()),
        name: Some(TOOL.into()),
        arguments: Some(r#"{"prompt": ""#.into()),
    }];
    events.extend((2..pieces).map(|_| LlmStreamEvent::ToolCallDelta {
        index: 0,
        id: None,
        name: None,
        arguments: Some("a ".into()),
    }));
    events.push(LlmStreamEvent::ToolCallDelta {
        index: 0,
        id: None,
        name: None,
        arguments: Some(r#""}"#.into()),
    });
    let done = LlmStreamEvent::Done {
        finish_reason: Some("tool_calls".into()),
    };
    let spaced = stream::iter(events).then(move |event| async move {
        tokio::time::sleep(apart).await;
        Ok(event)
    });
    Box::pin(spaced.chain(stream::iter([Ok(done)])))
}

/// The reports a reply sent while it was collected, read as they come so
/// that no number of them can fill the channel, and that its call arrived
/// whole.
async fn reports(events: Events) -> Vec<ToolCallWriting> {
    let (tx, mut rx) = mpsc::channel(64);
    let collect = async move { collect_stream(events, &tx).await.unwrap() };
    let receive = async {
        let mut sent = Vec::new();
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::ToolCallWriting(writing) => sent.push(writing),
                other => panic!("only writing reports are sent: {other:?}"),
            }
        }
        sent
    };
    let (response, sent) = tokio::join!(collect, receive);
    assert_eq!(response.tool_calls.len(), 1);
    sent
}

/// A thousand pieces with no time between them are one report, sent for
/// the first piece and naming the tool.
#[tokio::test(start_paused = true)]
async fn a_call_written_at_once_is_reported_once_naming_its_tool() {
    let sent = reports(written(1000, Duration::ZERO)).await;
    assert_eq!(
        sent,
        [ToolCallWriting {
            tool_name: TOOL.to_owned(),
            display_name: "Generate Image".to_owned(),
            tokens: 1,
        }]
    );
}

/// Ten pieces 300 ms apart are reported at the first, then at the first
/// piece a second or more after each report: the 5th and the 9th.
#[tokio::test(start_paused = true)]
async fn a_call_written_slowly_is_reported_once_a_second() {
    let sent = reports(written(10, Duration::from_millis(300))).await;
    let tokens: Vec<u32> = sent.iter().map(|writing| writing.tokens).collect();
    assert_eq!(tokens, [1, 5, 9]);
}
