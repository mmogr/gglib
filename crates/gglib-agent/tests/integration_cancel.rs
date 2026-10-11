//! A run that is dropped while a tool call is in flight drops the call.
//!
//! Cancelling a run drops the loop's future (`gglib-axum`'s run does that on
//! a Stop). The tool calls of a batch run in tasks of their own, so what a
//! Stop reaches depends on those tasks ending with the loop: a tool that
//! holds something for as long as it runs, as the drawing tool holds a
//! render, lets go of it only when its own future is dropped.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentMessage, ToolCall, ToolDefinition, ToolResult};
use gglib_core::ports::ToolExecutorPort;
use serde_json::json;
use tokio::sync::mpsc;

/// Counts itself when dropped.
struct Dropped(Arc<AtomicUsize>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// One tool, `draw`, that never answers: it counts each call it is given and
/// each call that was dropped unanswered.
struct NeverAnswers {
    started: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolExecutorPort for NeverAnswers {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition::new("draw")]
    }

    async fn execute(&self, _call: &ToolCall) -> Result<ToolResult> {
        let _held = Dropped(Arc::clone(&self.dropped));
        self.started.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
    }
}

/// Wait, for at most five seconds, until `count` reads `want`.
async fn reaches(count: &AtomicUsize, want: usize) -> bool {
    for _ in 0..500 {
        if count.load(Ordering::SeqCst) == want {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
async fn a_run_dropped_mid_tool_drops_the_tool_call_and_starts_no_other() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call("tc1", "draw", json!({})))
            .push(MockLlmResponse::text("never reached")),
    );
    let (started, dropped) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let executor = NeverAnswers {
        started: Arc::clone(&started),
        dropped: Arc::clone(&dropped),
    };
    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, _rx) = mpsc::channel(64);
    let run = tokio::spawn(async move {
        agent
            .run(
                vec![AgentMessage::user("draw a fox")],
                AgentConfig::default(),
                tx,
            )
            .await
    });
    assert!(reaches(&started, 1).await, "the tool call is in flight");
    assert_eq!(dropped.load(Ordering::SeqCst), 0, "and still held");

    run.abort();
    assert!(run.await.unwrap_err().is_cancelled());

    assert!(
        reaches(&dropped, 1).await,
        "the call in flight was dropped with the run"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(started.load(Ordering::SeqCst), 1, "no second call was made");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}
