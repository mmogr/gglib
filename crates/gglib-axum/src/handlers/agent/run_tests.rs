//! An agent run's frames are each event's JSON, and it saves nothing
//! without a conversation.

use axum::response::IntoResponse as _;
use gglib_core::domain::agent::{
    AgentEvent, INCOMPLETE_KEY, THINKING_DURATION_KEY, THINKING_KEY, ToolCall,
};
use gglib_core::domain::chat::MessageRole;
use gglib_core::domain::runs::{RunInfo, RunKind, RunStatus};
use http_body_util::BodyExt as _;
use serde_json::json;

use super::compose::frame;
use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, meta, paced, prepared, saved, settled, start, state,
};
use gglib_core::ports::RunsPort as _;

async fn body(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// The `data:` lines of an SSE body, less the run's closing event.
fn data_lines(body: &str) -> Vec<String> {
    body.split("\n\n")
        .filter(|event| !event.starts_with("event: run"))
        .flat_map(str::lines)
        .filter(|line| line.starts_with("data:"))
        .map(str::to_owned)
        .collect()
}

/// A frame's own bytes, as the page and the CLI parse them: a change to the
/// framing must show here.
#[test]
fn a_frame_is_these_bytes() {
    let events = [
        AgentEvent::TextDelta {
            content: "hi \"there\"\n".to_owned(),
        },
        AgentEvent::ToolCallStart {
            tool_call: ToolCall {
                id: "c1".to_owned(),
                name: "read_file".to_owned(),
                arguments: json!({ "path": "a.rs" }),
            },
            display_name: "Read File".to_owned(),
            args_summary: None,
            server: None,
        },
    ];

    let frames: Vec<String> = events.iter().map(frame).collect();

    assert_eq!(
        frames,
        [
            r#"{"type":"text_delta","content":"hi \"there\"\n"}"#,
            r#"{"type":"tool_call_start","tool_call":{"id":"c1","name":"read_file","arguments":{"path":"a.rs"}},"display_name":"Read File","args_summary":null}"#,
        ]
    );
}

#[tokio::test]
async fn a_runs_frames_are_its_events_and_it_ends_completed() {
    let (_dir, state) = state().await;
    let (p, _) = prepared(finished_reply(), End::Finish);

    let info = start(&state, "a1", None, p).await;
    assert_eq!(
        (info.kind, info.model.as_deref()),
        (RunKind::Agent, Some("qwen"))
    );
    let events = state.runs.events(&LOCAL, "a1", 0).unwrap();
    let run = body(gglib_proxy::runs::sse::stream(events, None).into_response()).await;

    let frames: Vec<String> = finished_reply()
        .iter()
        .map(|event| format!("data: {}", frame(event)))
        .collect();
    assert_eq!(data_lines(&run), frames);
    let end = run
        .split("event: run\ndata: ")
        .nth(1)
        .expect("the run's end");
    let end: RunInfo = serde_json::from_str(end.trim_end()).unwrap();
    assert_eq!(end.status, RunStatus::Completed);
    settled(&state).await;
    assert_eq!(state.agent_semaphore.available_permits(), 1);
}

#[tokio::test]
async fn a_completed_run_saves_the_users_message_then_the_reply() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, _) = prepared(finished_reply(), End::Finish);

    let info = start(&state, "a1", Some(id), p).await;
    assert_eq!(info.conversation_id, Some(id));
    settled(&state).await;

    let rows = saved(&state, id).await;
    let roles: Vec<MessageRole> = rows.iter().map(|r| r.role).collect();
    assert_eq!(
        roles,
        [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Tool,
            MessageRole::Assistant
        ]
    );
    assert_eq!(rows[0].content, "PROMPT-SECRET");
    assert_eq!(meta(&rows[1], THINKING_KEY), json!("REASON-SECRET"));
    assert_eq!(meta(&rows[1], "tool_calls")[0]["id"], "c1");
    assert_eq!(meta(&rows[2], "tool_call_id"), json!("c1"));
    assert_eq!(rows[3].content, "ANSWER-SECRET");
    assert!(rows.iter().all(|r| meta(r, INCOMPLETE_KEY).is_null()));
}

#[tokio::test]
async fn with_no_conversation_nothing_is_saved() {
    let (_dir, state) = state().await;
    let canary = conversation(&state).await;
    let (p, _) = prepared(finished_reply(), End::Finish);

    let info = start(&state, "a1", None, p).await;
    settled(&state).await;

    assert_eq!(info.conversation_id, None);
    assert!(saved(&state, canary).await.is_empty());
    let conversations = state
        .core
        .chat_history()
        .list_conversations()
        .await
        .unwrap();
    assert_eq!(conversations.len(), 1);
}

/// How long the model thought is saved where the page reads it, measured
/// from when the reasoning's first and last events were logged.
#[tokio::test]
async fn a_turn_that_reasoned_saves_how_long_it_thought() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let reasoning = |c: &str| AgentEvent::ReasoningDelta {
        content: c.to_owned(),
    };
    let events = vec![
        reasoning("a"),
        reasoning("b"),
        AgentEvent::FinalAnswer {
            content: "done".to_owned(),
        },
    ];
    let (p, _) = paced(events, End::Finish, std::time::Duration::from_millis(250));

    start(&state, "a1", Some(id), p).await;
    settled(&state).await;

    let rows = saved(&state, id).await;
    let seconds = meta(&rows[1], THINKING_DURATION_KEY)
        .as_f64()
        .expect("a duration");
    assert!((0.2..3.0).contains(&seconds), "{seconds}");
    assert_eq!(meta(&rows[1], THINKING_KEY), json!("ab"));
}
