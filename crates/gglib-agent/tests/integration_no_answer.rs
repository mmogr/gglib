//! A final reply that is no answer ends the run as
//! [`AgentError::NoAnswer`], after an `error` event that says so.
//!
//! A reply with no words is never an answer. A reply that starts with a
//! tool call written as text is none on a turn that offered no tools, which
//! is where a model that tries a call anyway writes one; with tools offered
//! it is the answer, as any text is. What the model wrote is sent before
//! the reply is judged: its text, and its thinking as thinking.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use common::event_assertions::{collect_events, has_tool_complete_with_success};
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage, ToolDefinition};
use gglib_core::ports::{AgentError, AgentRunOutput};
use serde_json::json;
use tokio::sync::mpsc;

const SEARCH: &str = "search";

/// What the run's `error` event and its error both say.
const NO_ANSWER: &str = "The model gave no answer.";

const TOOL_CALL: &str = "<tool_call>\n{\"name\": \"search\", \"arguments\": {}}\n</tool_call>";
const BRACKETED: &str = "[TOOL_CALL]\n{tool => \"search\", args => {}}\n[/TOOL_CALL]";
const CALL_THEN_WORDS: &str = "\n<tool_call>{\"name\": \"search\"}</tool_call>\nI will look.";
const WORDS_NAMING_A_CALL: &str = "The parser reads `<tool_call>` blocks in src/parse.rs.";
const WORDS_THEN_CALL: &str = "It is in src/lib.rs.\n<tool_call>{\"name\": \"search\"}</tool_call>";

/// Run a model that gives `replies` in turn, offered the `search` tool or,
/// with `tools_offered` false, none: how the run ended, and its events.
async fn run(
    replies: Vec<MockLlmResponse>,
    tools_offered: bool,
) -> (Result<AgentRunOutput, AgentError>, Vec<AgentEvent>) {
    let llm = Arc::new(MockLlmPort::new().push_many(replies));
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new(SEARCH),
        MockToolBehavior::Immediate {
            content: "found it".into(),
        },
    );
    // An empty filter offers no tool.
    let filter = (!tools_offered).then(HashSet::new);
    let agent = AgentLoop::build(llm, Arc::new(executor), filter);
    let (tx, rx) = mpsc::channel(64);

    let ended = agent
        .run(
            vec![AgentMessage::user("find it")],
            AgentConfig::default(),
            tx,
        )
        .await;

    (ended, collect_events(rx).await)
}

/// A reply of thinking alone.
fn thought(reasoning: &str) -> MockLlmResponse {
    MockLlmResponse {
        reasoning: Some(reasoning.to_owned()),
        content: None,
        tool_calls: vec![],
        finish_reason: "stop".into(),
    }
}

/// That the run ended as no answer: its last event the `error` that says
/// so, the only one, and no `final_answer`.
fn assert_no_answer(ended: &Result<AgentRunOutput, AgentError>, events: &[AgentEvent], of: &str) {
    assert!(
        matches!(ended, Err(AgentError::NoAnswer)),
        "{of:?}: expected NoAnswer, got: {ended:?}"
    );
    assert!(
        matches!(events.last(), Some(AgentEvent::Error { message, .. }) if message == NO_ANSWER),
        "{of:?}: the error event is last: {events:?}"
    );
    let errors = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Error { .. }))
        .count();
    assert_eq!(errors, 1, "{of:?}: one error event: {events:?}");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::FinalAnswer { .. })),
        "{of:?}: no final answer: {events:?}"
    );
}

/// Whether `events` hold `text` as a text delta.
fn sent_as_text(events: &[AgentEvent], text: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, AgentEvent::TextDelta { content } if content == text))
}

#[tokio::test]
async fn a_final_reply_with_no_words_is_no_answer_whether_or_not_tools_were_offered() {
    for reply in ["", " \n"] {
        for tools_offered in [true, false] {
            let (ended, events) = run(vec![MockLlmResponse::text(reply)], tools_offered).await;

            assert_no_answer(&ended, &events, reply);
        }
    }
}

#[tokio::test]
async fn a_reply_of_thinking_alone_is_no_answer_and_its_thinking_is_still_sent() {
    for tools_offered in [true, false] {
        let (ended, events) = run(vec![thought("It may be in src/lib.rs.")], tools_offered).await;

        assert_no_answer(&ended, &events, "thinking alone");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::ReasoningDelta { content }
                if content == "It may be in src/lib.rs.")),
            "the thinking is sent as thinking: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, AgentEvent::TextDelta { .. })),
            "none of it is sent as text: {events:?}"
        );
    }
}

/// The text is sent as it is written, before the reply can be judged, so a
/// reader has it when the run then ends as no answer.
#[tokio::test]
async fn a_final_reply_that_starts_with_a_call_in_text_is_no_answer_when_no_tools_were_offered() {
    for reply in [TOOL_CALL, BRACKETED, CALL_THEN_WORDS] {
        let (ended, events) = run(vec![MockLlmResponse::text(reply)], false).await;

        assert_no_answer(&ended, &events, reply);
        assert!(sent_as_text(&events, reply), "{reply:?} in {events:?}");
    }
}

#[tokio::test]
async fn a_final_reply_that_starts_with_a_call_in_text_is_the_answer_when_tools_were_offered() {
    for reply in [TOOL_CALL, BRACKETED, CALL_THEN_WORDS] {
        let (ended, events) = run(vec![MockLlmResponse::text(reply)], true).await;

        assert_eq!(ended.expect(reply).answer, reply);
        assert!(
            matches!(events.last(), Some(AgentEvent::FinalAnswer { content }) if content == reply),
            "{reply:?}: {events:?}"
        );
    }
}

#[tokio::test]
async fn a_final_reply_whose_words_come_first_is_the_answer_whether_or_not_tools_were_offered() {
    for reply in [WORDS_NAMING_A_CALL, WORDS_THEN_CALL] {
        for tools_offered in [true, false] {
            let (ended, _) = run(vec![MockLlmResponse::text(reply)], tools_offered).await;

            assert_eq!(ended.expect(reply).answer, reply);
        }
    }
}

/// The tools a run made before a final reply with no words ran, and their
/// events were sent: the run ends as no answer after them.
#[tokio::test]
async fn a_final_reply_with_no_words_after_tools_ran_is_no_answer() {
    let replies = vec![
        MockLlmResponse::tool_call("c1", SEARCH, json!({})),
        MockLlmResponse::text(""),
    ];

    let (ended, events) = run(replies, true).await;

    assert_no_answer(&ended, &events, "after a tool round");
    assert!(has_tool_complete_with_success(&events, true), "{events:?}");
}

/// A reply through the proxy whose stream ended with no finish frame and
/// showed nothing arrives as the proxy's notice, then a `Done` with no
/// reason. The notice is not the model's reply, so the run is no answer.
#[tokio::test]
async fn a_reply_that_is_only_the_proxys_notice_is_no_answer() {
    use gglib_core::LlmStreamEvent;

    let notice = "⚠️ [proxy] The model produced no output for this request. The upstream server \
                  may be overloaded or degraded — retry, and if it persists restart the model. \
                  (finish_reason: none)";
    let llm = Arc::new(MockLlmPort::new().push_events(vec![
        LlmStreamEvent::TextDelta {
            content: notice.into(),
        },
        LlmStreamEvent::Done {
            finish_reason: None,
        },
    ]));
    let agent = AgentLoop::build(llm, Arc::new(MockToolExecutorPort::new()), None);
    let (tx, rx) = mpsc::channel(64);

    let ended = agent
        .run(
            vec![AgentMessage::user("find it")],
            AgentConfig::default(),
            tx,
        )
        .await;
    let events = collect_events(rx).await;

    assert_no_answer(&ended, &events, "the proxy's notice");
    assert!(!sent_as_text(&events, notice), "{events:?}");
}
