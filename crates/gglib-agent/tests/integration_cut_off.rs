//! A reply the upstream cut off at its token limit (`finish_reason:
//! "length"`). One cut off while it wrote a tool call, or before it wrote
//! any text, ends the run with `AgentError::TurnTooLong` and one `error`
//! event: a call in it is not run and no further request is made. One cut
//! off in its text is taken as if it had stopped, after one cut-off
//! `system_warning`. The words after a demanded call that has run are the
//! exception: one cut off with no text, or while it wrote a call, is
//! answered with the composer's words and the warning.

mod common;

use std::sync::Arc;

use common::event_assertions::collect_events;
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::{AgentLoop, is_cut_off_warning};
use gglib_core::domain::agent::{
    AgentEvent, AgentMessage, FirstCall, LlmStreamEvent, ToolDefinition,
};
use gglib_core::normalize::NormalizationErrorKind;
use gglib_core::ports::{AgentError, AgentRunOutput};
use serde_json::json;
use tokio::sync::mpsc;

const TOOL: &str = "builtin:generate_image";
const WORDLESS: &str = "The model said nothing.";

/// `events`, then the upstream cutting the reply off after `tokens` tokens.
fn cut_after(mut events: Vec<LlmStreamEvent>, tokens: u32) -> Vec<LlmStreamEvent> {
    events.extend([
        LlmStreamEvent::Done {
            finish_reason: Some("length".into()),
        },
        LlmStreamEvent::Usage {
            prompt_tokens: 474,
            completion_tokens: tokens,
            total_tokens: 474 + tokens,
            cached_tokens: None,
        },
    ]);
    events
}

/// A delta of a call to the tool at `index`, cut off inside its arguments.
fn call_delta(index: usize) -> LlmStreamEvent {
    LlmStreamEvent::ToolCallDelta {
        index,
        id: Some("call_1".into()),
        name: Some(TOOL.into()),
        arguments: Some(r#"{"prompt": "a T rex, a T rex, a T rex"#.into()),
    }
}

/// A reply that starts a call to the tool and is cut off inside its
/// arguments, after `tokens` tokens.
fn cut_off_inside_a_call(tokens: u32) -> Vec<LlmStreamEvent> {
    cut_after(vec![call_delta(0)], tokens)
}

fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.into(),
    }
}

/// A reply that only thought, cut off after `tokens` tokens.
fn cut_off_thinking(tokens: u32) -> Vec<LlmStreamEvent> {
    let thought = LlmStreamEvent::ReasoningDelta {
        content: "Let me think about the T rex".into(),
    };
    cut_after(vec![thought], tokens)
}

/// `response` with `finish_reason: "length"`.
fn cut_off(mut response: MockLlmResponse) -> MockLlmResponse {
    response.finish_reason = "length".into();
    response
}

/// A run over `llm` with the one tool, held to a first call of it when
/// `first` says: how it ended, its events, and the calls the tool got.
async fn run(
    llm: &Arc<MockLlmPort>,
    first: bool,
) -> (
    Result<AgentRunOutput, AgentError>,
    Vec<AgentEvent>,
    Vec<(String, serde_json::Value)>,
) {
    let drew = MockToolBehavior::Immediate {
        content: "Drew 1 image.".to_owned(),
    };
    let executor = MockToolExecutorPort::new().with_tool(ToolDefinition::new(TOOL), drew);
    let ran = Arc::clone(&executor.call_log);
    let agent = AgentLoop::build(Arc::clone(llm) as _, Arc::new(executor), None);
    let first_call = first.then(|| FirstCall {
        tool: TOOL.to_owned(),
        if_missing: "the model did not ask for the picture".to_owned(),
        if_wordless: WORDLESS.to_owned(),
        then_say: "Say what you drew.".to_owned(),
        then_say_failed: "Say what went wrong.".to_owned(),
    });
    let config = common::for_test(|c| c.first_call = first_call);
    let (tx, rx) = mpsc::channel(64);
    let ended = agent
        .run(vec![AgentMessage::user("draw a T rex")], config, tx)
        .await;
    let ran = ran.lock().await.clone();
    (ended, collect_events(rx).await, ran)
}

/// The messages of every `error` event, in order.
fn errors(events: &[AgentEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Error { message } => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

/// The messages of every cut-off `system_warning`, in order.
fn cut_off_warnings(events: &[AgentEvent]) -> Vec<&str> {
    events
        .iter()
        .filter(|event| is_cut_off_warning(event))
        .filter_map(|event| match event {
            AgentEvent::SystemWarning { message, .. } => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

/// The text of the run's `final_answer`, when it sent one.
fn answer(events: &[AgentEvent]) -> Option<&str> {
    events.iter().find_map(|event| match event {
        AgentEvent::FinalAnswer { content } => Some(content.as_str()),
        _ => None,
    })
}

/// A run that ended as a turn too long: one `error`, no answer, no
/// warning, no call run and one request, on a plain run and on one held to
/// a first call.
async fn assert_too_long(reply: Vec<LlmStreamEvent>, tokens: Option<u32>) {
    for first in [false, true] {
        let llm = Arc::new(
            MockLlmPort::new()
                .push_events(reply.clone())
                .push(MockLlmResponse::text("never asked")),
        );
        let (ended, events, ran) = run(&llm, first).await;

        assert!(
            matches!(ended, Err(AgentError::TurnTooLong { completion_tokens }) if completion_tokens == tokens),
            "first call: {first}: {ended:?}"
        );
        assert_eq!(errors(&events).len(), 1, "first call: {first}: {events:?}");
        assert_eq!(answer(&events), None, "first call: {first}");
        assert!(cut_off_warnings(&events).is_empty(), "first call: {first}");
        assert!(ran.is_empty(), "first call: {first}: {ran:?}");
        assert_eq!(
            llm.asked().await.len(),
            1,
            "first call: {first}: no second request"
        );
    }
}

/// The incident's shape: a call that never closed. It ends the run as a
/// turn that ran too long, saying how long, not as malformed arguments.
#[tokio::test]
async fn a_call_cut_off_mid_argument_ends_the_run_as_too_long() {
    assert_too_long(cut_off_inside_a_call(16_384), Some(16_384)).await;
    let llm = Arc::new(MockLlmPort::new().push_events(cut_off_inside_a_call(16_384)));
    let (_, events, _) = run(&llm, false).await;
    let errors = errors(&events);
    assert!(errors[0].contains("ran too long"), "{errors:?}");
    assert!(errors[0].contains("after 16384 tokens"), "{errors:?}");
}

/// A call whose arguments happen to close is still cut off: it is not run,
/// and the run does not go on to a second request.
#[tokio::test]
async fn a_whole_call_in_a_cut_off_reply_is_not_run() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(cut_off(MockLlmResponse::tool_call(
                "call_1",
                TOOL,
                json!({ "prompt": "a T rex" }),
            )))
            .push(MockLlmResponse::text("never asked")),
    );
    let (ended, events, ran) = run(&llm, false).await;

    assert!(
        matches!(
            ended,
            Err(AgentError::TurnTooLong {
                completion_tokens: None
            })
        ),
        "{ended:?}"
    );
    assert_eq!(errors(&events).len(), 1);
    assert!(ran.is_empty(), "{ran:?}");
    assert_eq!(llm.asked().await.len(), 1, "no second request");
}

/// Words before a call do not make the reply an answer: it was writing
/// the call when it was cut off.
#[tokio::test]
async fn text_and_a_cut_off_call_end_the_run_as_too_long() {
    let reply = cut_after(vec![text("Drawing it now."), call_delta(0)], 900);
    assert_too_long(reply, Some(900)).await;
}

/// A call past the slots the collector keeps is still a call being
/// written when the reply is cut off.
#[tokio::test]
async fn a_cut_off_call_past_the_kept_slots_ends_the_run_as_too_long() {
    let reply = cut_after(
        vec![
            text("Drawing it now."),
            call_delta(gglib_agent::MAX_TOOL_CALL_INDEX),
        ],
        900,
    );
    assert_too_long(reply, Some(900)).await;
}

/// A dialect model's call comes as markup, which its parser holds back: a
/// reply cut off inside it streams no call delta, only the parser's report
/// of a tool-call tag left open. That is still a call being written.
#[tokio::test]
async fn a_cut_off_dialect_call_ends_the_run_as_too_long() {
    let reply = cut_after(
        vec![
            text("Sure, drawing it now: "),
            LlmStreamEvent::NormalizationError {
                kind: NormalizationErrorKind::UnclosedToolCallTag {
                    partial: r#"{"name": "generate_image", "arguments": {"prompt": "a T"#.into(),
                },
                raw: "<tool_call>".into(),
            },
        ],
        900,
    );
    assert_too_long(reply, Some(900)).await;
}

/// A reply cut off while it only thought has no answer to give.
#[tokio::test]
async fn a_cut_off_reply_that_only_thought_ends_the_run_as_too_long() {
    assert_too_long(cut_off_thinking(16_384), Some(16_384)).await;
}

/// Text cut off at the limit is the answer, as it was before the limit, on
/// a plain run and on the words that follow a demanded call; the run says
/// once that it was cut off, and how long it ran.
#[tokio::test]
async fn cut_off_words_are_the_answer_with_a_warning() {
    let plain =
        Arc::new(MockLlmPort::new().push_events(cut_after(vec![text("The T rex was a")], 512)));
    let after_the_call = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "call_1",
                TOOL,
                json!({ "prompt": "a T rex" }),
            ))
            .push_events(cut_after(vec![text("I drew a T rex that")], 512)),
    );
    for (llm, first, said, requests) in [
        (plain, false, "The T rex was a", 1),
        (after_the_call, true, "I drew a T rex that", 2),
    ] {
        let (ended, events, _) = run(&llm, first).await;

        let output = ended.unwrap_or_else(|e| panic!("first call: {first}: {e:?}"));
        assert_eq!(output.answer, said, "first call: {first}");
        assert_eq!(answer(&events), Some(said), "first call: {first}");
        assert!(
            errors(&events).is_empty(),
            "first call: {first}: {events:?}"
        );
        let warnings = cut_off_warnings(&events);
        assert_eq!(warnings.len(), 1, "first call: {first}: {events:?}");
        assert!(warnings[0].contains("after 512 tokens"), "{warnings:?}");
        assert_eq!(llm.asked().await.len(), requests, "first call: {first}");
    }
}

/// The words after a demanded call that has run, cut off before they said
/// anything: the call's work is done, so the run does not fail. It answers
/// with the composer's words, and says the answer was cut off.
#[tokio::test]
async fn cut_off_wordless_words_after_the_call_are_answered_for() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "call_1",
                TOOL,
                json!({ "prompt": "a T rex" }),
            ))
            .push_events(cut_off_thinking(8192)),
    );
    let (ended, events, ran) = run(&llm, true).await;

    let output = ended.unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(output.answer, WORDLESS);
    assert_eq!(answer(&events), Some(WORDLESS));
    assert!(errors(&events).is_empty(), "{events:?}");
    assert_eq!(cut_off_warnings(&events).len(), 1, "{events:?}");
    assert_eq!(ran.len(), 1, "the demanded call ran: {ran:?}");
}

/// The words after the call, cut off while they wrote another call, do not
/// fail a run whose demanded call has run: that call is not run, and the
/// run answers with the composer's words and the warning.
#[tokio::test]
async fn words_cut_off_inside_a_call_after_the_call_are_answered_for() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "call_1",
                TOOL,
                json!({ "prompt": "a T rex" }),
            ))
            .push_events(cut_off_inside_a_call(8192)),
    );
    let (ended, events, ran) = run(&llm, true).await;

    let output = ended.unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(output.answer, WORDLESS);
    assert!(errors(&events).is_empty(), "{events:?}");
    assert_eq!(cut_off_warnings(&events).len(), 1, "{events:?}");
    assert_eq!(ran.len(), 1, "only the demanded call ran: {ran:?}");
}

/// The words after the call, with `cut` (text and then a call) cut off: the
/// text led into the call and is not the answer, so the run answers with
/// the composer's words and the warning, and runs only the demanded call.
async fn assert_words_with_text_cut_in_a_call_are_answered_for(cut: Vec<LlmStreamEvent>) {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "call_1",
                TOOL,
                json!({ "prompt": "a T rex" }),
            ))
            .push_events(cut_after(cut, 8192)),
    );
    let (ended, events, ran) = run(&llm, true).await;

    let output = ended.unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(output.answer, WORDLESS);
    assert_eq!(answer(&events), Some(WORDLESS));
    assert!(errors(&events).is_empty(), "{events:?}");
    assert_eq!(cut_off_warnings(&events).len(), 1, "{events:?}");
    assert_eq!(ran.len(), 1, "only the demanded call ran: {ran:?}");
}

#[tokio::test]
async fn words_with_text_cut_off_inside_a_call_are_answered_for() {
    assert_words_with_text_cut_in_a_call_are_answered_for(vec![
        text("Now drawing another:"),
        call_delta(0),
    ])
    .await;
}

/// The same for a dialect model, whose call is markup left open.
#[tokio::test]
async fn words_with_text_cut_off_inside_a_dialect_call_are_answered_for() {
    assert_words_with_text_cut_in_a_call_are_answered_for(vec![
        text("Sure, drawing it now: "),
        LlmStreamEvent::NormalizationError {
            kind: NormalizationErrorKind::UnclosedToolCallTag {
                partial: r#"{"name": "generate_image", "arguments": {"prompt": "a T"#.into(),
            },
            raw: "<tool_call>".into(),
        },
    ])
    .await;
}

/// The cut-off warning on the wire, as docs/clients.md gives it to clients.
#[test]
fn the_cut_off_warning_is_a_system_warning_with_a_hint() {
    assert_eq!(
        serde_json::to_value(gglib_agent::cut_off_warning(Some(512))).unwrap(),
        json!({
            "type": "system_warning",
            "message": "The answer was cut off at its token limit after 512 tokens, before it finished.",
            "suggested_action": "Raise max_tokens to give it more room.",
        })
    );
    assert_eq!(
        serde_json::to_value(gglib_agent::cut_off_warning(None)).unwrap()["message"],
        "The answer was cut off at its token limit, before it finished."
    );
}
