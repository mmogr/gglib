//! A run that uses all its steps (`AgentConfig::max_iterations`).
//!
//! Such a run sends a `system_warning`, then makes one closing request with
//! no tools, ending with a note that its steps are used up. A reply with text
//! is the run's final answer, also when it was cut off at its token limit. A
//! request that fails, a reply with no text (cut off or not) and a reply that
//! starts with tool-call markup end the run with `MaxIterationsReached`, as
//! before. A run with a demanded first call makes no closing request.

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

/// The steps each run here has.
const STEPS: usize = 2;

/// The note the closing request ends with, for [`STEPS`].
const NOTE: &str = "You have used all 2 of this run's steps and can make no more tool calls. \
     Give your answer now from what you have found. If you cannot answer yet, say what you have \
     established and what is left to do, and ask for more steps.";

/// One run that calls a tool at each of its [`STEPS`] steps, then gets
/// `closing` (nothing: the request fails) to its closing request: how it
/// ended, its events, the model, and how many tool calls ran.
async fn run_to_the_limit(
    closing: Option<MockLlmResponse>,
) -> (
    Result<AgentRunOutput, AgentError>,
    Vec<AgentEvent>,
    Arc<MockLlmPort>,
    usize,
) {
    let mut llm =
        MockLlmPort::new()
            .push_many((0..STEPS).map(|i| {
                MockLlmResponse::tool_call(format!("tc{i}"), "search", json!({ "i": i }))
            }));
    if let Some(reply) = closing {
        llm = llm.push(reply);
    }
    let llm = Arc::new(llm);
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate {
            content: "a finding".into(),
        },
    );
    let ran = Arc::clone(&executor.call_log);
    let agent = AgentLoop::build(Arc::clone(&llm) as _, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(128);
    let config = common::for_test(|c| c.max_iterations = STEPS);

    let ended = agent
        .run(vec![AgentMessage::user("find it")], config, tx)
        .await;

    let events = collect_events(rx).await;
    let ran = ran.lock().await.len();
    (ended, events, llm, ran)
}

/// Where the step-limit warning is in `events`.
fn warning_at(events: &[AgentEvent]) -> Option<usize> {
    events.iter().position(|e| {
        matches!(e, AgentEvent::SystemWarning { message, .. }
            if message == "This run used all 2 of its steps. It was asked to answer without \
                    further tool calls.")
    })
}

/// How many `error` events `events` holds.
fn errors(events: &[AgentEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Error { .. }))
        .count()
}

/// Whether `message` is the closing request's note.
fn is_note(message: &AgentMessage) -> bool {
    matches!(message, AgentMessage::User { content, .. } if content == NOTE)
}

/// That the model was asked once more than [`STEPS`], the last time with no
/// tools, no demanded call, and the note last.
async fn assert_closing_request_made(llm: &MockLlmPort) {
    let asked = llm.asked().await;
    assert_eq!(asked.len(), STEPS + 1, "one closing request: {asked:?}");
    assert_eq!(asked[STEPS], (vec![], false), "no tools offered");
    let sent = llm.messages_received().await;
    assert!(sent[STEPS].last().is_some_and(is_note), "the note is last");
}

/// That the run ended on the limit, with one `error` event and no answer,
/// after the warning.
fn assert_ended_on_the_limit(ended: &Result<AgentRunOutput, AgentError>, events: &[AgentEvent]) {
    assert!(
        matches!(ended, Err(AgentError::MaxIterationsReached(STEPS))),
        "expected MaxIterationsReached({STEPS}), got: {ended:?}"
    );
    assert!(warning_at(events).is_some(), "the warning was sent");
    assert_eq!(errors(events), 1, "one error event: {events:?}");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::FinalAnswer { .. })),
        "no answer"
    );
}

#[tokio::test]
async fn a_closing_reply_with_text_is_the_runs_answer_after_the_warning() {
    let (ended, events, llm, ran) =
        run_to_the_limit(Some(MockLlmResponse::text("It is in src/lib.rs, line 4."))).await;

    let output = ended.expect("the run ends with the closing reply");
    assert_eq!(output.answer, "It is in src/lib.rs, line 4.");
    assert_eq!(
        output.total_iterations, STEPS,
        "the closing request is no step"
    );
    assert!(!output.history.iter().any(is_note), "the note is not kept");
    assert!(
        matches!(output.history.last(), Some(AgentMessage::Assistant { content })
            if content.text.as_deref() == Some("It is in src/lib.rs, line 4.")
                && content.tool_calls.is_empty()),
        "the history ends with the closing reply: {:?}",
        output.history.last()
    );
    assert_closing_request_made(&llm).await;
    assert_eq!(ran, STEPS);
    let warned = warning_at(&events).expect("the warning was sent");
    let answers: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, AgentEvent::FinalAnswer { .. }))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(answers.len(), 1, "one final answer: {events:?}");
    assert!(
        matches!(&events[answers[0]], AgentEvent::FinalAnswer { content } if content == &output.answer)
    );
    assert!(warned < answers[0], "the warning comes first");
    assert_eq!(errors(&events), 0, "no error event: {events:?}");
}

#[tokio::test]
async fn a_closing_reply_with_no_text_ends_the_run_on_the_limit() {
    let (ended, events, llm, _) = run_to_the_limit(Some(MockLlmResponse::text("  \n "))).await;

    assert_ended_on_the_limit(&ended, &events);
    assert_closing_request_made(&llm).await;
}

#[tokio::test]
async fn a_closing_request_that_fails_ends_the_run_on_the_limit() {
    // Nothing is scripted for the closing request, so the model's port fails it.
    let (ended, events, llm, _) = run_to_the_limit(None).await;

    assert_ended_on_the_limit(&ended, &events);
    assert_closing_request_made(&llm).await;
}

#[tokio::test]
async fn a_closing_reply_of_tool_call_markup_ends_the_run_on_the_limit() {
    let markup = "<tool_call>\n{\"name\": \"search\", \"arguments\": {}}\n</tool_call>";
    let (ended, events, llm, _) = run_to_the_limit(Some(MockLlmResponse::text(markup))).await;

    assert_ended_on_the_limit(&ended, &events);
    assert_closing_request_made(&llm).await;
}

/// A closing reply that holds `content` and was cut off at its token limit.
fn cut_off(content: Option<&str>) -> MockLlmResponse {
    MockLlmResponse {
        reasoning: None,
        content: content.map(str::to_owned),
        tool_calls: vec![],
        finish_reason: "length".into(),
    }
}

/// How many cut-off warnings `events` holds.
fn cut_off_warnings(events: &[AgentEvent]) -> usize {
    events
        .iter()
        .filter(|e| gglib_agent::is_cut_off_warning(e))
        .count()
}

#[tokio::test]
async fn a_closing_reply_cut_off_in_its_text_is_the_answer_with_both_warnings() {
    let (ended, events, _, _) = run_to_the_limit(Some(cut_off(Some("It is in src/lib")))).await;

    assert_eq!(
        ended.expect("the cut text is the answer").answer,
        "It is in src/lib"
    );
    assert!(warning_at(&events).is_some(), "the step-limit warning");
    assert_eq!(cut_off_warnings(&events), 1, "the cut-off warning");
    assert_eq!(errors(&events), 0, "no error event: {events:?}");
}

#[tokio::test]
async fn a_closing_reply_cut_off_before_any_text_ends_the_run_on_the_limit() {
    let (ended, events, llm, _) = run_to_the_limit(Some(cut_off(None))).await;

    assert_ended_on_the_limit(&ended, &events);
    assert_closing_request_made(&llm).await;
    assert_eq!(cut_off_warnings(&events), 1, "the cut-off warning");
    let error_at = events
        .iter()
        .position(|e| matches!(e, AgentEvent::Error { .. }))
        .expect("the error event");
    assert_eq!(error_at, events.len() - 1, "the error event is last");
}

#[tokio::test]
async fn a_closing_reply_that_names_markup_in_its_prose_is_the_answer() {
    let prose = "The parser reads `<tool_call>` blocks in src/parse.rs.";
    let (ended, _, _, _) = run_to_the_limit(Some(MockLlmResponse::text(prose))).await;

    assert_eq!(ended.expect("the prose is the answer").answer, prose);
}

#[tokio::test]
async fn a_tool_call_in_a_closing_reply_is_not_run() {
    let reply = MockLlmResponse {
        reasoning: None,
        content: Some("Here is what I found.".into()),
        tool_calls: vec![ToolCall {
            id: "late".into(),
            name: "search".into(),
            arguments: json!({}),
        }],
        finish_reason: "tool_calls".into(),
    };
    let (ended, _, _, ran) = run_to_the_limit(Some(reply)).await;

    assert_eq!(
        ended.expect("the text is the answer").answer,
        "Here is what I found."
    );
    assert_eq!(ran, STEPS, "only the run's own steps ran a tool");
}

#[tokio::test]
async fn a_run_with_a_demanded_first_call_makes_no_closing_request() {
    let llm =
        Arc::new(MockLlmPort::new().push(MockLlmResponse::tool_call("d1", "draw", json!({}))));
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("draw"),
        MockToolBehavior::Immediate {
            content: "Drew 1 image.".into(),
        },
    );
    let agent = AgentLoop::build(Arc::clone(&llm) as _, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);
    let config = common::for_test(|c| {
        c.max_iterations = 1;
        c.first_call = Some(FirstCall {
            tool: "draw".into(),
            if_missing: "no picture".into(),
            if_wordless: "no words".into(),
            then_say: "Say what you drew.".into(),
            then_say_failed: "Say what went wrong.".into(),
        });
    });

    let ended = agent
        .run(vec![AgentMessage::user("draw a fox")], config, tx)
        .await;

    let events = collect_events(rx).await;
    assert!(
        matches!(ended, Err(AgentError::MaxIterationsReached(1))),
        "got: {ended:?}"
    );
    assert_eq!(llm.asked().await.len(), 1, "no closing request");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::SystemWarning { .. })),
        "no warning"
    );
}
