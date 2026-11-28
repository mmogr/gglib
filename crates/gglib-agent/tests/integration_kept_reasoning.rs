//! A step's reasoning within one run. A loop built to keep it puts each
//! step's reasoning on that step's assistant message, so every later request
//! of the run carries it and the context budget counts it. A loop built
//! without (every model whose template is not known to keep reasoning) keeps
//! none and counts none. A step that wrote reasoning and neither text nor a
//! call is no message either way, and the run ends on it as it always has.

mod common;

use std::sync::Arc;

use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{
    AgentConfig, AgentEvent, AgentMessage, AssistantContent, ContextReading, ToolCall,
    ToolDefinition,
};
use gglib_core::ports::{AgentError, AgentLoopPort, AgentRunOutput};
use serde_json::json;
use tokio::sync::mpsc;

const THOUGHT: &str = "The bug is probably in parse(); read src/parse.rs first.";

fn search() -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: "search".into(),
        arguments: json!({}),
    }
}

/// One step that thinks `reasoning` and calls `search`.
fn thinks_and_calls(reasoning: &str) -> MockLlmResponse {
    MockLlmResponse {
        reasoning: Some(reasoning.to_owned()),
        ..MockLlmResponse::tool_call("c1", "search", json!({}))
    }
}

/// One step that thinks and writes nothing else, ending on `finish_reason`.
fn only_thinks(finish_reason: &str) -> MockLlmResponse {
    MockLlmResponse {
        reasoning: Some(THOUGHT.to_owned()),
        content: None,
        tool_calls: Vec::new(),
        finish_reason: finish_reason.to_owned(),
    }
}

fn executor() -> MockToolExecutorPort {
    MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate {
            content: "ok".into(),
        },
    )
}

fn agent(llm: &Arc<MockLlmPort>, keep_reasoning: bool) -> Arc<dyn AgentLoopPort> {
    let llm = Arc::clone(llm) as Arc<dyn gglib_core::ports::LlmCompletionPort>;
    AgentLoop::build_observed(llm, Arc::new(executor()), None, None, keep_reasoning)
}

/// Run `history` to its end with `config`: how it ended and its events.
async fn run(
    agent: &Arc<dyn AgentLoopPort>,
    history: Vec<AgentMessage>,
    config: AgentConfig,
) -> (Result<AgentRunOutput, AgentError>, Vec<AgentEvent>) {
    let (tx, mut rx) = mpsc::channel(256);
    let ended = agent.run(history, config, tx).await;
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    (ended, events)
}

/// The ids of the calls `step` holds.
fn ids(step: &AssistantContent) -> Vec<&str> {
    step.tool_calls
        .iter()
        .map(|call| call.id.as_str())
        .collect()
}

/// The assistant messages of one request.
fn assistants(request: &[AgentMessage]) -> Vec<&AssistantContent> {
    request
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Assistant { content } => Some(content),
            _ => None,
        })
        .collect()
}

/// The two-step run of the first check: a step that thinks and calls a
/// tool, then an answer. Answers what the model was sent on each request.
async fn two_steps(keep_reasoning: bool) -> Vec<Vec<AgentMessage>> {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(thinks_and_calls(THOUGHT))
            .push(MockLlmResponse::text("Fixed.")),
    );
    let (ended, _) = run(
        &agent(&llm, keep_reasoning),
        vec![AgentMessage::user("find the bug")],
        AgentConfig::default(),
    )
    .await;
    assert_eq!(ended.unwrap().answer, "Fixed.");
    llm.messages_received().await
}

/// Kept: the second request's assistant message carries the first step's
/// reasoning beside its call, and the size count includes it.
#[tokio::test]
async fn a_loop_that_keeps_reasoning_sends_the_first_steps_reasoning_back() {
    let sent = two_steps(true).await;

    assert_eq!(sent.len(), 2);
    assert!(assistants(&sent[0]).is_empty());
    let [step] = assistants(&sent[1])[..] else {
        panic!("one assistant message: {:?}", sent[1]);
    };
    assert_eq!(step.reasoning.as_deref(), Some(THOUGHT));
    assert_eq!(ids(step), ["c1"]);
    assert_eq!(step.text, None);
    let message = AgentMessage::Assistant {
        content: step.clone(),
    };
    let calls_alone = "c1".len() + "search".len() + "{}".len();
    assert_eq!(message.char_count(), calls_alone + THOUGHT.chars().count());
}

/// Not kept: the same run's second request carries no reasoning, and the
/// size count is the call's alone.
#[tokio::test]
async fn a_loop_that_does_not_keep_reasoning_sends_none_back() {
    let sent = two_steps(false).await;

    assert_eq!(sent.len(), 2);
    let [step] = assistants(&sent[1])[..] else {
        panic!("one assistant message: {:?}", sent[1]);
    };
    assert_eq!(step.reasoning, None);
    assert_eq!(ids(step), ["c1"]);
    let message = AgentMessage::Assistant {
        content: step.clone(),
    };
    assert_eq!(
        message.char_count(),
        "c1".len() + "search".len() + "{}".len()
    );
}

/// The trimmed count each model call's `turn_usage` carried, in call order.
fn trimmed(events: &[AgentEvent]) -> Vec<ContextReading> {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TurnUsage(usage) => Some(usage.reading),
            _ => None,
        })
        .collect()
}

/// Pruning counts what is sent. The history fits the budget with the step's
/// call alone and not with its reasoning too: a loop that keeps reasoning
/// prunes the old tool step before the second request, and one that keeps
/// none sends everything, as it does today.
#[tokio::test]
async fn kept_reasoning_counts_against_the_context_budget() {
    let long_thought = "r".repeat(300);
    let history = || {
        vec![
            AgentMessage::user("First question."),
            AgentMessage::Assistant {
                content: AssistantContent {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "old0".into(),
                        name: "search".into(),
                        arguments: json!({}),
                    }],
                    reasoning: None,
                },
            },
            AgentMessage::tool("old0", "x".repeat(100)),
            AgentMessage::user("And now?"),
        ]
    };
    let config = || {
        common::for_test(|c| {
            c.context_budget_chars = 400;
            c.prune_keep_tool_messages = 1;
        })
    };

    let mut readings = Vec::new();
    for keep in [true, false] {
        let llm = Arc::new(
            MockLlmPort::new()
                .push(thinks_and_calls(&long_thought))
                .push(MockLlmResponse::text("done")),
        );
        let (ended, events) = run(&agent(&llm, keep), history(), config()).await;
        ended.unwrap();
        let sent = llm.messages_received().await;
        readings.push((sent[1].len(), trimmed(&events)));
    }

    let [(kept_len, kept), (dropped_len, dropped)] = &readings[..] else {
        unreachable!()
    };
    assert_eq!(*dropped_len, 6, "nothing pruned without the reasoning");
    assert_eq!(dropped, &[ContextReading::default(); 2]);
    assert_eq!(*kept_len, 4, "the old call and its result were pruned");
    assert_eq!(
        kept,
        &[ContextReading::default(), ContextReading::new(None, 2)]
    );
}

/// A batch over the parallel limit is answered with an error for each call
/// and sent back; the step's reasoning goes back with it only on a loop
/// built to keep it.
#[tokio::test]
async fn an_overflowing_steps_reasoning_is_kept_too() {
    for keep in [true, false] {
        let two_calls = MockLlmResponse {
            reasoning: Some(THOUGHT.to_owned()),
            content: None,
            tool_calls: vec![
                search(),
                ToolCall {
                    id: "c2".into(),
                    ..search()
                },
            ],
            finish_reason: "tool_calls".into(),
        };
        let llm = Arc::new(
            MockLlmPort::new()
                .push(two_calls)
                .push(MockLlmResponse::text("done")),
        );
        let config = common::for_test(|c| c.max_parallel_tools = 1);

        let (ended, _) = run(&agent(&llm, keep), vec![AgentMessage::user("go")], config).await;

        ended.unwrap();
        let sent = llm.messages_received().await;
        let [step] = assistants(&sent[1])[..] else {
            panic!("keep {keep}: one assistant message: {:?}", sent[1]);
        };
        let expected = keep.then_some(THOUGHT);
        assert_eq!(step.reasoning.as_deref(), expected, "keep {keep}");
        assert_eq!(step.tool_calls.len(), 2, "keep {keep}");
    }
}

/// A step that thought and wrote neither text nor a call ends the run as it
/// does with reasoning not kept: cut off at its limit, with `TurnTooLong`;
/// stopped, with `NoAnswer`. No further request is made, so no message of
/// reasoning alone is ever sent.
#[tokio::test]
async fn a_step_of_reasoning_alone_ends_the_run_as_it_always_has() {
    for (finish_reason, cut) in [("length", true), ("stop", false)] {
        for keep in [true, false] {
            let llm = Arc::new(MockLlmPort::new().push(only_thinks(finish_reason)));
            let (ended, _) = run(
                &agent(&llm, keep),
                vec![AgentMessage::user("go")],
                AgentConfig::default(),
            )
            .await;
            let error = ended.expect_err("a step of reasoning alone is no answer");
            let as_expected = if cut {
                matches!(error, AgentError::TurnTooLong { .. })
            } else {
                matches!(error, AgentError::NoAnswer)
            };
            assert!(as_expected, "{finish_reason}, keep {keep}: {error:?}");
            assert_eq!(llm.messages_received().await.len(), 1, "no second request");
        }
    }
}

/// After a step that thought and called a tool, a step of reasoning alone
/// still ends the run: the kept message before it holds its call, and none
/// holds reasoning alone.
#[tokio::test]
async fn a_later_step_of_reasoning_alone_adds_no_message() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(thinks_and_calls(THOUGHT))
            .push(only_thinks("length")),
    );

    let (ended, _) = run(
        &agent(&llm, true),
        vec![AgentMessage::user("go")],
        AgentConfig::default(),
    )
    .await;

    assert!(
        matches!(ended, Err(AgentError::TurnTooLong { .. })),
        "{ended:?}"
    );
    let sent = llm.messages_received().await;
    assert_eq!(sent.len(), 2, "the run ended on the second step");
    for step in assistants(&sent[1]) {
        assert!(!step.tool_calls.is_empty() || step.text.is_some());
    }
}
