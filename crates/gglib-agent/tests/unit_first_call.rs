//! A run whose first reply must call one tool (`AgentConfig::first_call`).
//!
//! The loop offers that tool alone on its first call to the model and
//! demands a call; every later call offers the run's whole list and demands
//! nothing. A first reply that does not call that tool ends the run with the
//! composer's words, only the reply's first call of it is kept, and a run
//! with no first call asks as it always did.

mod common;

use std::sync::Arc;

use common::event_assertions::collect_events;
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentEvent, AgentMessage, FirstCall, ToolCall, ToolDefinition};
use gglib_core::ports::{AgentError, AgentRunOutput};
use serde_json::json;
use tokio::sync::mpsc;

const DRAW: &str = "builtin:generate_image";
const READ: &str = "3:read_file";
const NOT_ASKED: &str = "the model did not ask for the picture";

fn first_call(tool: &str) -> FirstCall {
    FirstCall {
        tool: tool.to_owned(),
        if_missing: NOT_ASKED.to_owned(),
    }
}

/// One run over `llm` with the two tools, held to `first_call`: how it
/// ended, its events, and the tools it ran.
async fn run(
    llm: &Arc<MockLlmPort>,
    first_call: Option<FirstCall>,
) -> (
    Result<AgentRunOutput, AgentError>,
    Vec<AgentEvent>,
    Vec<(String, serde_json::Value)>,
) {
    let ok = |content: &str| MockToolBehavior::Immediate {
        content: content.to_owned(),
    };
    let executor = MockToolExecutorPort::new()
        .with_tool(ToolDefinition::new(DRAW), ok("Drew 1 image."))
        .with_tool(ToolDefinition::new(READ), ok("contents"));
    let ran = Arc::clone(&executor.call_log);
    let agent = AgentLoop::build(Arc::clone(llm) as _, Arc::new(executor), None);
    let config = common::for_test(|c| c.first_call = first_call);
    let (tx, rx) = mpsc::channel(64);

    let ended = agent
        .run(vec![AgentMessage::user("draw a red fox")], config, tx)
        .await;

    let events = collect_events(rx).await;
    let ran = ran.lock().await.clone();
    (ended, events, ran)
}

/// The names of the tools that `ran`, in order.
fn names(ran: &[(String, serde_json::Value)]) -> Vec<&str> {
    ran.iter().map(|(name, _)| name.as_str()).collect()
}

/// One reply that makes each of `calls`, given as (id, tool, prompt).
fn reply_calling(calls: &[(&str, &str, &str)]) -> MockLlmResponse {
    let call = |&(id, name, prompt): &(&str, &str, &str)| ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments: json!({ "prompt": prompt }),
    };
    MockLlmResponse {
        reasoning: None,
        content: None,
        tool_calls: calls.iter().map(call).collect(),
        finish_reason: "tool_calls".into(),
    }
}

/// The ids of the calls the model is shown as having made, and of the
/// results it is shown, in the request numbered `request`.
async fn shown(llm: &MockLlmPort, request: usize) -> (Vec<String>, Vec<String>) {
    let sent = llm.messages_received().await;
    let mut calls = Vec::new();
    let mut results = Vec::new();
    for message in &sent[request] {
        match message {
            AgentMessage::Assistant { content } => {
                calls.extend(content.tool_calls.iter().map(|call| call.id.clone()));
            }
            AgentMessage::Tool { tool_call_id, .. } => results.push(tool_call_id.clone()),
            _ => {}
        }
    }
    (calls, results)
}

/// Whether the tools `offered` are exactly `names`, in any order.
fn offers(offered: &[String], names: &[&str]) -> bool {
    offered.len() == names.len() && names.iter().all(|name| offered.iter().any(|o| o == name))
}

/// The first call offers the demanded tool alone and demands a call; the
/// second offers every tool and demands nothing, so the model can answer.
#[tokio::test]
async fn the_first_call_offers_one_tool_and_demands_it_and_the_next_is_free() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "c1",
                DRAW,
                json!({"prompt": "a fox"}),
            ))
            .push(MockLlmResponse::text("A red fox in the snow.")),
    );

    let (ended, _events, ran) = run(&llm, Some(first_call(DRAW))).await;

    assert_eq!(ended.unwrap().answer, "A red fox in the snow.");
    let asked = llm.asked().await;
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0], (vec![DRAW.to_owned()], true));
    assert!(offers(&asked[1].0, &[DRAW, READ]), "{:?}", asked[1]);
    assert!(!asked[1].1, "only the first call is demanded");
    assert_eq!(names(&ran), [DRAW]);
}

/// A run with no first call asks as it always did: every tool, no demand.
#[tokio::test]
async fn a_run_with_no_first_call_demands_nothing() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call("c1", READ, json!({})))
            .push(MockLlmResponse::text("done")),
    );

    let (ended, _events, _ran) = run(&llm, None).await;

    assert!(ended.is_ok());
    let asked = llm.asked().await;
    assert_eq!(asked.len(), 2);
    for (offered, demanded) in &asked {
        assert!(offers(offered, &[DRAW, READ]), "{offered:?}");
        assert!(!demanded);
    }
}

/// A first reply that calls nothing ends the run there: the composer's
/// words are its last `error` event and its error, the text is no answer,
/// no tool ran and the model is not asked again.
#[tokio::test]
async fn a_first_reply_with_no_call_ends_the_run_with_the_composers_words() {
    let fenced = "```json\n{\"name\": \"builtin:generate_image\"}\n```";
    let llm = Arc::new(MockLlmPort::new().push(MockLlmResponse::text(fenced)));

    let (ended, events, ran) = run(&llm, Some(first_call(DRAW))).await;

    match ended {
        Err(AgentError::FirstCallMissing { message }) => assert_eq!(message, NOT_ASKED),
        other => panic!("expected the missing first call, got {other:?}"),
    }
    assert_eq!(llm.asked().await.len(), 1);
    assert!(ran.is_empty());
    let answered = |event: &AgentEvent| matches!(event, AgentEvent::FinalAnswer { .. });
    assert!(!events.iter().any(answered), "the text was no answer");
    let errors: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Error { message } => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(errors, [NOT_ASKED]);
}

/// A first call for a tool the run was never given fails before the model
/// is asked anything: nothing could satisfy it.
#[tokio::test]
async fn a_first_call_for_a_tool_the_run_lacks_fails_before_the_model_is_asked() {
    let llm = Arc::new(MockLlmPort::new().push(MockLlmResponse::text("unreached")));

    let (ended, _events, _ran) = run(&llm, Some(first_call("builtin:not_there"))).await;

    assert!(matches!(ended, Err(AgentError::Internal(_))), "{ended:?}");
    assert!(llm.asked().await.is_empty());
}

/// A first reply that calls some other tool is no call of the demanded one:
/// the run ends as it does for a reply with no call, and that tool never
/// runs.
#[tokio::test]
async fn a_first_reply_that_calls_another_tool_ends_the_run_with_the_composers_words() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(reply_calling(&[("c1", READ, "a fox")]))
            .push(MockLlmResponse::text("unreached")),
    );

    let (ended, events, ran) = run(&llm, Some(first_call(DRAW))).await;

    match ended {
        Err(AgentError::FirstCallMissing { message }) => assert_eq!(message, NOT_ASKED),
        other => panic!("expected the missing first call, got {other:?}"),
    }
    assert_eq!(llm.asked().await.len(), 1);
    assert!(ran.is_empty(), "{ran:?}");
    let said =
        |event: &AgentEvent| matches!(event, AgentEvent::Error { message } if message == NOT_ASKED);
    assert!(events.iter().any(said), "{events:?}");
}

/// A first reply that calls the demanded tool more than once runs it once,
/// from the reply's first call, and the next request shows the model that
/// one call and its one result.
#[tokio::test]
async fn a_first_reply_that_calls_the_tool_twice_runs_its_first_call_alone() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(reply_calling(&[
                ("c1", DRAW, "a fox"),
                ("c2", DRAW, "a hen"),
            ]))
            .push(MockLlmResponse::text("A red fox.")),
    );

    let (ended, _events, ran) = run(&llm, Some(first_call(DRAW))).await;

    assert_eq!(ended.unwrap().answer, "A red fox.");
    assert_eq!(ran, [(DRAW.to_owned(), json!({ "prompt": "a fox" }))]);
    let (calls, results) = shown(&llm, 1).await;
    assert_eq!(calls, ["c1"]);
    assert_eq!(results, ["c1"]);
}

/// A first reply that calls another tool beside the demanded one runs the
/// demanded one alone, wherever in the reply it stands, and the next
/// request shows the model no call that has no result.
#[tokio::test]
async fn a_first_reply_that_calls_another_tool_too_runs_the_demanded_one_alone() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(reply_calling(&[
                ("c1", READ, "a den"),
                ("c2", DRAW, "a fox"),
            ]))
            .push(MockLlmResponse::text("A red fox.")),
    );

    let (ended, _events, ran) = run(&llm, Some(first_call(DRAW))).await;

    assert_eq!(ended.unwrap().answer, "A red fox.");
    assert_eq!(ran, [(DRAW.to_owned(), json!({ "prompt": "a fox" }))]);
    let (calls, results) = shown(&llm, 1).await;
    assert_eq!(calls, ["c2"]);
    assert_eq!(results, ["c2"]);
}
