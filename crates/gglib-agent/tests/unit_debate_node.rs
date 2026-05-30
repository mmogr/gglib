//! Unit tests for the debate node executor.
//!
//! Uses a stub [`LlmCompletionPort`] that returns canned responses so no real
//! LLM server is required.  All tests run in CI without `#[ignore]`.
//!
//! # What is tested
//!
//! - Happy path: 2 agents, 1 round, no judge — correct set of events emitted.
//! - Cancellation: pre-cancelled token causes `DebateError::Cancelled` without
//!   any LLM calls.

#![allow(unused_crate_dependencies)]

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use futures_util::stream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use gglib_agent::council::debate::{DebateError, run_debate_node};
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::task_graph::{DebateAgent, DebateConfig};
use gglib_core::ports::{EmptyToolExecutor, LlmCompletionPort, ResponseFormat, ToolExecutorPort};
use gglib_core::{AgentConfig, AgentMessage, LlmStreamEvent, ToolDefinition};

// =============================================================================
// Stub LLM
// =============================================================================

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
// Helpers
// =============================================================================

fn two_agent_config() -> DebateConfig {
    DebateConfig {
        agents: vec![
            DebateAgent {
                id: "agent_a".to_owned(),
                name: "Agent A".to_owned(),
                color: "#FF0000".to_owned(),
                persona: "You are a cautious analyst.".to_owned(),
                perspective: "risk-averse".to_owned(),
                contentiousness: 0.3,
                tool_filter: None,
            },
            DebateAgent {
                id: "agent_b".to_owned(),
                name: "Agent B".to_owned(),
                color: "#0000FF".to_owned(),
                persona: "You are an optimistic strategist.".to_owned(),
                perspective: "growth-focused".to_owned(),
                contentiousness: 0.7,
                tool_filter: None,
            },
        ],
        rounds: 1,
        judge: None,
        synthesis_guidance: None,
    }
}

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

/// Happy path: 2 agents, 1 round, no judge.
///
/// LLM call order:
///   1. Agent A turn              → plain text (no core-claim markers → stance skipped)
///   2. Agent B turn              → plain text
///   3. Synthesis                 → plain text
///
/// Expected events (minimum):
///   DebateRoundStarted × 1
///   DebateAgentTurnStarted × 2
///   DebateAgentTurnComplete × 2
///   DebateSynthesisStarted × 1
///   DebateSynthesisComplete × 1
#[tokio::test]
async fn debate_two_agents_one_round_emits_expected_events() {
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done("Agent A says: we should be cautious."),
        StubLlm::text_then_done("Agent B says: we should grow aggressively."),
        StubLlm::text_then_done("Synthesis: a balanced approach is best."),
    ]);
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(256);
    let cancel = CancellationToken::new();
    let config = two_agent_config();

    let result = run_debate_node(
        "node-1",
        "Should we expand to new markets?",
        "",
        &config,
        Arc::new(llm),
        tool_executor,
        &AgentConfig::default(),
        &tx,
        cancel,
    )
    .await;

    drop(tx);
    let events = collect_events(rx).await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let round_started = events
        .iter()
        .filter(|e| matches!(e, CouncilEvent::DebateRoundStarted { .. }))
        .count();
    assert_eq!(round_started, 1, "expected 1 DebateRoundStarted");

    let turn_started = events
        .iter()
        .filter(|e| matches!(e, CouncilEvent::DebateAgentTurnStarted { .. }))
        .count();
    assert_eq!(turn_started, 2, "expected 2 DebateAgentTurnStarted");

    let turn_complete = events
        .iter()
        .filter(|e| matches!(e, CouncilEvent::DebateAgentTurnComplete { .. }))
        .count();
    assert_eq!(turn_complete, 2, "expected 2 DebateAgentTurnComplete");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, CouncilEvent::DebateSynthesisStarted { .. })),
        "missing DebateSynthesisStarted"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CouncilEvent::DebateSynthesisComplete { .. })),
        "missing DebateSynthesisComplete"
    );

    // Synthesis text should be non-empty.
    let synthesis_text = result.unwrap();
    assert!(
        !synthesis_text.is_empty(),
        "synthesis text should not be empty"
    );
}

/// Agent turns carry the correct round number (1-based).
#[tokio::test]
async fn debate_round_number_is_one_based() {
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done("Agent A."),
        StubLlm::text_then_done("Agent B."),
        StubLlm::text_then_done("Synthesis."),
    ]);
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(256);
    let config = two_agent_config();

    let _ = run_debate_node(
        "node-round",
        "topic",
        "",
        &config,
        Arc::new(llm),
        tool_executor,
        &AgentConfig::default(),
        &tx,
        CancellationToken::new(),
    )
    .await;

    drop(tx);
    let events = collect_events(rx).await;

    for e in &events {
        if let CouncilEvent::DebateRoundStarted { round, .. } = e {
            assert_eq!(*round, 1, "DebateRoundStarted round should be 1-based");
        }
        if let CouncilEvent::DebateAgentTurnStarted { round, .. } = e {
            assert_eq!(*round, 1, "DebateAgentTurnStarted round should be 1-based");
        }
    }
}

/// Pre-cancelled token returns `DebateError::Cancelled` immediately without
/// making any LLM calls.
#[tokio::test]
async fn debate_cancelled_before_start_returns_cancelled() {
    let llm = StubLlm::new(vec![]); // no responses queued — would panic if called
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, _rx) = mpsc::channel(256);
    let cancel = CancellationToken::new();
    cancel.cancel();

    let result = run_debate_node(
        "node-cancel",
        "topic",
        "",
        &two_agent_config(),
        Arc::new(llm),
        tool_executor,
        &AgentConfig::default(),
        &tx,
        cancel,
    )
    .await;

    assert!(
        matches!(result, Err(DebateError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
}
