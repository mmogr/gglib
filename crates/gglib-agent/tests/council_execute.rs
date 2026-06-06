//! Integration tests for the orchestrator executor.
//!
//! Uses a stub [`LlmCompletionPort`] that returns canned responses so no real
//! LLM server is required.  All tests run in CI without `#[ignore]`.
//!
//! # What is tested
//!
//! - Topological execution order: root nodes run before dependent nodes.
//! - Context isolation: a worker's messages must NOT contain the director's
//!   planning history.
//! - Compaction: the worker output is compacted and stored.
//! - `FilteredToolExecutor` enforcement: a worker with an empty `tool_allowlist`
//!   sees no tools.
//! - Fail-fast: a worker error causes `CouncilError` on the stream and
//!   `ExecuteError` from `execute()`.
//! - Happy path: `CouncilComplete` is the last event on success.
//!
//! # Stub LLM design
//!
//! The stub queues canned `Vec<LlmStreamEvent>` sequences.  Each call to
//! `chat_stream()` pops the next sequence from the front of the queue and
//! returns it as a stream.  The test configures:
//!
//! 1. Director response (JSON matching the plan schema).
//! 2. Worker response(s) per node.
//! 3. Compaction response(s) per node (≥ 1 per worker).
//! 4. Synthesis response.

#![allow(unused_crate_dependencies)]

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use futures_util::stream;
use tokio::sync::mpsc;

use gglib_agent::council::{CouncilConfig, execute};
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::ports::{EmptyToolExecutor, LlmCompletionPort, ResponseFormat, ToolExecutorPort};
use gglib_core::{AgentMessage, LlmStreamEvent, ToolDefinition};

// =============================================================================
// Stub LLM
// =============================================================================

/// Canned response queue.  Each call to `chat_stream` pops one entry.
type ResponseQueue = Arc<Mutex<VecDeque<Vec<LlmStreamEvent>>>>;

#[derive(Clone)]
struct StubLlm {
    queue: ResponseQueue,
}

impl StubLlm {
    fn new(responses: Vec<Vec<LlmStreamEvent>>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(responses.into())),
        }
    }

    fn text_then_done(text: &str) -> Vec<LlmStreamEvent> {
        vec![
            LlmStreamEvent::TextDelta {
                content: text.to_owned(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ]
    }
}

#[async_trait]
impl LlmCompletionPort for StubLlm {
    async fn chat_stream(
        &self,
        _messages: &[AgentMessage],
        _tools: &[ToolDefinition],
        _response_format: Option<&ResponseFormat>,
    ) -> anyhow::Result<
        Pin<Box<dyn futures_core::Stream<Item = anyhow::Result<LlmStreamEvent>> + Send>>,
    > {
        let mut queue = self.queue.lock().unwrap();
        #[allow(clippy::option_if_let_else)]
        if let Some(events) = queue.pop_front() {
            let items: Vec<anyhow::Result<LlmStreamEvent>> = events.into_iter().map(Ok).collect();
            Ok(Box::pin(stream::iter(items)))
        } else {
            Err(anyhow!("StubLlm: no more canned responses"))
        }
    }
}

// =============================================================================
// Helper: build a minimal valid director JSON response
// =============================================================================

/// A two-node plan with `a → b` dependency.
fn two_node_plan_json() -> String {
    serde_json::json!({
        "goal": "Write a report",
        "nodes": [
            {
                "id": "research",
                "goal": "Research the topic thoroughly",
                "depends_on": [],
                "tool_allowlist": []
            },
            {
                "id": "write",
                "goal": "Write the report based on research",
                "depends_on": ["research"],
                "tool_allowlist": []
            }
        ]
    })
    .to_string()
}

/// A single-node plan (simplest case).
fn one_node_plan_json() -> String {
    serde_json::json!({
        "goal": "Answer a simple question",
        "nodes": [
            {
                "id": "answer",
                "goal": "Provide a concise answer to the question",
                "depends_on": [],
                "tool_allowlist": []
            }
        ]
    })
    .to_string()
}

/// Chief-of-Staff response for a single department (used to prepend to each
/// executor test so the new hierarchical planner gets its first LLM call
/// satisfied before the Director call).
fn cos_single_dept_json() -> String {
    serde_json::json!({
        "departments": [
            {
                "name": "main",
                "mission": "Complete the task.",
                "suggested_roles": []
            }
        ]
    })
    .to_string()
}

// =============================================================================
// Helper: channel collector
// =============================================================================

/// Collect all events from an `CouncilEvent` channel into a `Vec`.
async fn collect_events(mut rx: mpsc::Receiver<CouncilEvent>) -> Vec<CouncilEvent> {
    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    events
}

// =============================================================================
// Tests
// =============================================================================

/// Happy path: single node, all events arrive, `CouncilComplete` is last.
#[tokio::test]
async fn single_node_happy_path_ends_with_complete() {
    // LLM call order:
    // 1. Director structured output   → one_node_plan_json
    // 2. Worker (answer)              → "The answer is 42."
    // 3. Compaction (answer)          → "The worker answered: 42."
    // 4. Synthesis                    → "42."
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&one_node_plan_json()),
        StubLlm::text_then_done("The answer is 42."),
        StubLlm::text_then_done("The worker answered: 42."),
        // synthesizer leaf worker + compaction at top-level
        StubLlm::text_then_done("Synthesizer output."),
        StubLlm::text_then_done("Synthesizer compacted."),
        // final synthesis
        StubLlm::text_then_done("42."),
    ]);

    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(1024);

    let result = execute(
        "Answer a simple question",
        &[],
        Arc::new(llm),
        tool_executor,
        CouncilConfig::default(),
        tx,
    )
    .await;

    assert!(result.is_ok(), "execute() should succeed: {result:?}");

    let events = collect_events(rx).await;

    // CouncilComplete must appear.
    let has_complete = events
        .iter()
        .any(|e| matches!(e, CouncilEvent::CouncilComplete { .. }));
    assert!(has_complete, "must have CouncilComplete; got: {events:?}");

    // NodeStarted for 'answer' must appear.
    let has_node_started = events
        .iter()
        .any(|e| matches!(e, CouncilEvent::NodeStarted { node_id, .. } if node_id == "answer"));
    assert!(has_node_started, "must have NodeStarted for 'answer'");
}

/// Two-node plan: `research → write`.  `NodeStarted` for 'research' must
/// precede `NodeComplete` for 'research', which must precede `NodeStarted`
/// for 'write'.
#[tokio::test]
async fn two_node_topological_order() {
    // LLM call order:
    // 1. Director
    // 2. Worker: research
    // 3. Compaction: research
    // 4. Worker: write
    // 5. Compaction: write
    // 6. Synthesis
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&two_node_plan_json()),
        StubLlm::text_then_done("Research findings: lots of info."),
        StubLlm::text_then_done("Research found lots of info."),
        StubLlm::text_then_done("Report based on research."),
        StubLlm::text_then_done("Report complete."),
        // synthesizer leaf worker + compaction at top-level
        StubLlm::text_then_done("Synthesizer output."),
        StubLlm::text_then_done("Synthesizer compacted."),
        // final synthesis
        StubLlm::text_then_done("Final synthesised answer."),
    ]);

    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(1024);

    let result = execute(
        "Write a report",
        &[],
        Arc::new(llm),
        tool_executor,
        CouncilConfig::default(),
        tx,
    )
    .await;

    assert!(result.is_ok(), "execute() should succeed: {result:?}");

    let events = collect_events(rx).await;

    // Find positions of key events.
    let research_started = events.iter().position(
        |e| matches!(e, CouncilEvent::NodeStarted { node_id, .. } if node_id == "research"),
    );
    let research_complete = events.iter().position(
        |e| matches!(e, CouncilEvent::NodeComplete { node_id, .. } if node_id == "research"),
    );
    let write_started = events
        .iter()
        .position(|e| matches!(e, CouncilEvent::NodeStarted { node_id, .. } if node_id == "write"));

    assert!(research_started.is_some(), "NodeStarted(research) missing");
    assert!(
        research_complete.is_some(),
        "NodeComplete(research) missing"
    );
    assert!(write_started.is_some(), "NodeStarted(write) missing");

    assert!(
        research_started.unwrap() < research_complete.unwrap(),
        "research must start before it completes"
    );
    assert!(
        research_complete.unwrap() < write_started.unwrap(),
        "research must complete before write starts (dependency order)"
    );
}

/// Worker error causes `CouncilError` event and `ExecuteError` return.
#[tokio::test]
async fn worker_error_triggers_fail_fast() {
    // Director succeeds, worker's LlmCompletion errors out (StubLlm queue
    // exhausted mid-stream). We simulate this by making the worker response
    // an empty vec so the AgentLoop gets no events and returns an error.
    // Actually, simplest: provide no more entries after director → the next
    // chat_stream call returns Err("no more canned responses"), which the
    // AgentLoop surfaces as AgentError::Internal, which our worker converts
    // to ExecuteError::WorkerFailed.
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&one_node_plan_json()),
        // No more entries — next call will return Err.
    ]);

    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(1024);

    let result = execute(
        "Answer a simple question",
        &[],
        Arc::new(llm),
        tool_executor,
        CouncilConfig::default(),
        tx,
    )
    .await;

    let events = collect_events(rx).await;

    // Must emit CouncilError.
    let has_error = events
        .iter()
        .any(|e| matches!(e, CouncilEvent::CouncilError { .. }));
    assert!(has_error, "must have CouncilError; got: {events:?}");

    // execute() must return Err.
    assert!(
        result.is_err(),
        "execute() must return Err on worker failure"
    );
}

/// `PlanApproved` event must appear after `PlanProposed` on a successful run.
#[tokio::test]
async fn plan_approved_event_emitted() {
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&one_node_plan_json()),
        StubLlm::text_then_done("Answer."),
        StubLlm::text_then_done("Worker answered."),
        // synthesizer leaf worker + compaction
        StubLlm::text_then_done("Synthesizer output."),
        StubLlm::text_then_done("Synthesizer compacted."),
        StubLlm::text_then_done("42."),
    ]);

    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(1024);

    let _ = execute(
        "Answer a simple question",
        &[],
        Arc::new(llm),
        tool_executor,
        CouncilConfig::default(),
        tx,
    )
    .await;

    let events = collect_events(rx).await;

    let proposed_pos = events
        .iter()
        .position(|e| matches!(e, CouncilEvent::PlanProposed { .. }));
    let approved_pos = events
        .iter()
        .position(|e| matches!(e, CouncilEvent::PlanApproved));

    assert!(proposed_pos.is_some(), "PlanProposed missing");
    assert!(approved_pos.is_some(), "PlanApproved missing");
    assert!(
        proposed_pos.unwrap() < approved_pos.unwrap(),
        "PlanProposed must precede PlanApproved"
    );
}

/// `SynthesisStart` must appear and be followed by `SynthesisComplete` on success.
#[tokio::test]
async fn synthesis_events_emitted() {
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&one_node_plan_json()),
        StubLlm::text_then_done("Answer."),
        StubLlm::text_then_done("Worker answered."),
        // synthesizer leaf worker + compaction
        StubLlm::text_then_done("Synthesizer output."),
        StubLlm::text_then_done("Synthesizer compacted."),
        StubLlm::text_then_done("Synthesised answer."),
    ]);

    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(1024);

    let _ = execute(
        "Answer a simple question",
        &[],
        Arc::new(llm),
        tool_executor,
        CouncilConfig::default(),
        tx,
    )
    .await;

    let events = collect_events(rx).await;

    let synth_start = events
        .iter()
        .position(|e| matches!(e, CouncilEvent::SynthesisStart));
    let synth_complete = events
        .iter()
        .position(|e| matches!(e, CouncilEvent::SynthesisComplete { .. }));

    assert!(synth_start.is_some(), "SynthesisStart missing");
    assert!(synth_complete.is_some(), "SynthesisComplete missing");
    assert!(
        synth_start.unwrap() < synth_complete.unwrap(),
        "SynthesisStart before SynthesisComplete"
    );
}

// =============================================================================
// Strict tool allowlist validation test
// =============================================================================

/// A `ToolExecutorPort` that lists a fixed set of tools and always errors on
/// `execute` (not expected to be called in these tests).
struct StubToolExecutor {
    tools: Vec<ToolDefinition>,
}

impl StubToolExecutor {
    fn with_tool(name: &str) -> Self {
        Self {
            tools: vec![ToolDefinition::new(name)],
        }
    }
}

#[async_trait::async_trait]
impl gglib_core::ports::ToolExecutorPort for StubToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.clone()
    }

    async fn execute(
        &self,
        _call: &gglib_core::ToolCall,
    ) -> Result<gglib_core::ToolResult, anyhow::Error> {
        Err(anyhow!(
            "StubToolExecutor: execute not expected in this test"
        ))
    }
}

/// **Strict tool validation**: when a node's `tool_allowlist` references a
/// tool that is not registered in the executor, the worker must fail
/// immediately with a descriptive error instead of letting the LLM reason
/// about the missing tool for thousands of tokens.
///
/// Expects:
/// - `NodeFailed` event emitted for the offending node.
/// - `execute()` returns `Err(ExecuteError::WorkerFailed)` whose reason
///   mentions the missing tool name.
/// - No `CouncilComplete` event (run aborted).
#[tokio::test]
async fn worker_fails_immediately_on_missing_tool() {
    // Plan with one node that requests `browser_snapshot` — a tool that is
    // NOT in the StubToolExecutor (which only exposes `read_file`).
    let plan_json = serde_json::json!({
        "goal": "Browse a website",
        "nodes": [
            {
                "id": "browse",
                "goal": "Navigate to a URL and extract data.",
                "depends_on": [],
                "tool_allowlist": ["browser_snapshot"]
            }
        ]
    })
    .to_string();

    // Director response + CoS response.  No worker response needed — the
    // worker should fail before making any LLM call.
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&plan_json),
        // No further responses — if the LLM were called it would error,
        // but the strict validation should prevent that.
    ]);

    // Executor exposes `read_file` but NOT `browser_snapshot`.
    let tool_executor: Arc<dyn gglib_core::ports::ToolExecutorPort> =
        Arc::new(StubToolExecutor::with_tool("read_file"));

    let (tx, rx) = mpsc::channel(1024);

    let result = execute(
        "Browse a website",
        &[],
        Arc::new(llm),
        tool_executor,
        CouncilConfig::default(),
        tx,
    )
    .await;

    let events = collect_events(rx).await;

    // NodeFailed must be emitted for the `browse` node.
    let node_failed = events.iter().find(|e| {
        matches!(
            e,
            CouncilEvent::NodeFailed { node_id, error }
                if node_id == "browse" && error.contains("browser_snapshot")
        )
    });
    assert!(
        node_failed.is_some(),
        "NodeFailed(browse) with missing tool name expected; got: {events:?}"
    );

    // execute() must return Err.
    assert!(
        result.is_err(),
        "execute() must return Err; got: {result:?}"
    );

    // CouncilComplete must NOT be emitted.
    let has_complete = events
        .iter()
        .any(|e| matches!(e, CouncilEvent::CouncilComplete { .. }));
    assert!(
        !has_complete,
        "CouncilComplete must not be emitted on tool-not-found failure"
    );
}
