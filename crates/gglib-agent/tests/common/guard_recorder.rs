//! A recording [`AgentGuardSink`] and the scaffolding around it (#1091).
//!
//! Beside the other mocks rather than in the one suite that uses it, because
//! the suite it is for has no room left in the file-size budget, and because
//! the next path to report its guard decisions will want the same recorder.

use std::sync::{Arc, Mutex};

use gglib_core::domain::agent::{AgentConfig, AgentMessage, ToolDefinition};
use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::ports::{
    AgentError, AgentGuardReporter, AgentGuardSink, AgentLoopPort, AgentRunOutput,
};
use serde_json::json;
use tokio::sync::mpsc;

use super::mock_llm::{MockLlmPort, MockLlmResponse};
use super::mock_tools::{MockToolBehavior, MockToolExecutorPort};

/// The model name every reporter built here counts under.
pub(crate) const MODEL: &str = "qwen3";

/// Records every decision in order.
///
/// In order, not merely counted: a total cannot tell one trip and one quiet
/// turn from two of either, and the sequence is what says the tripping turn
/// was the last one.
#[derive(Default)]
pub(crate) struct Recorder {
    calls: Mutex<Vec<(String, Option<LoopGuardTrip>)>>,
}

impl Recorder {
    pub(crate) fn calls(&self) -> Vec<(String, Option<LoopGuardTrip>)> {
        self.calls.lock().expect("recorder lock").clone()
    }

    pub(crate) fn trips(&self) -> Vec<LoopGuardTrip> {
        self.calls().into_iter().filter_map(|(_, t)| t).collect()
    }
}

impl AgentGuardSink for Recorder {
    fn record_decision(&self, model: &str, trip: Option<LoopGuardTrip>) {
        self.calls
            .lock()
            .expect("recorder lock")
            .push((model.to_owned(), trip));
    }
}

/// A reporter pointed at `recorder`, counting under [`MODEL`].
pub(crate) fn reporter(recorder: &Arc<Recorder>) -> AgentGuardReporter {
    AgentGuardReporter {
        sink: Arc::clone(recorder) as Arc<dyn AgentGuardSink>,
        model: MODEL.to_owned(),
    }
}

/// An executor whose one tool always gives the same answer back.
pub(crate) fn unchanging_executor() -> MockToolExecutorPort {
    MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("do_thing"),
        MockToolBehavior::Immediate {
            content: "the same answer".into(),
        },
    )
}

/// Six identical tool-call turns: enough to trip `max_repeated_batch_steps`.
pub(crate) fn repeating_llm() -> Arc<MockLlmPort> {
    Arc::new(MockLlmPort::new().push_many(
        (0..6).map(|i| MockLlmResponse::tool_call(format!("tc{i}"), "do_thing", json!({}))),
    ))
}

/// One tool-call turn, then a final answer.
pub(crate) fn one_call_then_answer() -> Arc<MockLlmPort> {
    Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call("tc0", "do_thing", json!({})))
            .push(MockLlmResponse::text("done")),
    )
}

/// Drive `agent` to completion on a one-message conversation.
pub(crate) async fn run_to_end(
    agent: &Arc<dyn AgentLoopPort>,
    config: AgentConfig,
) -> Result<AgentRunOutput, AgentError> {
    let (tx, _rx) = mpsc::channel(128);
    agent
        .run(
            vec![AgentMessage::User {
                content: "go".into(),
            }],
            config,
            tx,
        )
        .await
}
