//! What a turn on a hub chat sends this machine's endpoint, which it reaches
//! as any client does: the bearer key, the model's catalogue id, the
//! conversation unshaped, the person's own choices, the first call's demand
//! and the turn's limit. The endpoint folds the sampling ladder, trims,
//! writes the dialect's grammar and parses the dialect's markup; what comes
//! back is read as the `OpenAI` reply it already is.

use gglib_core::domain::agent::{AgentEvent, AgentMessage};
use gglib_core::domain::{DefaultsOrigin, InferenceConfig};
use gglib_core::normalize::tags::FORMAT_QWEN_XML;
use serde_json::json;

use crate::handlers::agent::run_fixture::{drawing_state, state};
use crate::handlers::agent::turn_fixture::{
    DRAWS, ENDPOINT_KEY, REPLY, TOOL, global, model, offers_nothing, phone, sampling_of, sent,
    stored, temperature, turn,
};

/// The key and the model's id travel; the sampling the ladder resolves does
/// not, since the endpoint folds it: here a stored global temperature and
/// the model's own top-p. A turn's limit does, the endpoint taking it as the
/// client's own budget.
#[tokio::test]
async fn a_turn_carries_the_key_and_the_models_id_and_no_folded_sampling() {
    let (_dir, state) = state().await;
    let own = InferenceConfig {
        top_p: Some(0.8),
        ..InferenceConfig::default()
    };
    let id = model(&state, stored(own, DefaultsOrigin::User)).await;
    global(&state, temperature(0.42)).await;
    let chat = phone(&state, id).await;

    let done = turn(&state, id, chat, REPLY).await;

    assert!(done.ended_well, "{:?}", done.events);
    assert_eq!(done.bearers, [Some(format!("Bearer {ENDPOINT_KEY}"))]);
    let body = &done.requests[0];
    assert_eq!(body["model"], json!(id.to_string()));
    let sampling = sampling_of(body);
    assert_eq!(sampling.temperature, None, "{body}");
    assert_eq!(sampling.top_p, None, "{body}");
    assert_eq!(body["max_tokens"], json!(16_384));
}

/// A `max_tokens` the endpoint's ladder names, a stored setting here, is
/// left for the endpoint: a limit from this side would outrank it.
#[tokio::test]
async fn a_limit_a_setting_names_is_left_for_the_endpoint() {
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;
    let limit = InferenceConfig {
        max_tokens: Some(65_536),
        ..InferenceConfig::default()
    };
    global(&state, limit).await;

    let body = sent(&state, id, phone(&state, id).await).await;

    assert!(body.get("max_tokens").is_none(), "{body}");
}

/// Thinking off is `reasoning_budget_tokens: 0` on the wire, which the
/// endpoint honours from any client, whatever it trusts.
#[tokio::test]
async fn thinking_off_reaches_the_endpoint() {
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;
    let mut chat = phone(&state, id).await;
    chat.reasoning_budget_tokens = Some(0);

    let body = sent(&state, id, chat).await;

    assert_eq!(body["reasoning_budget_tokens"], json!(0));
}

/// Draw's first request demands the call, offering the image tool alone,
/// and its second offers nothing: the endpoint reads `required` as it reads
/// any client's.
#[tokio::test]
async fn a_draw_demands_its_call_through_the_endpoint() {
    let (_dir, state) = drawing_state().await;
    let id = model(&state, |_| {}).await;
    let mut chat = phone(&state, id).await;
    chat.draw = true;
    chat.tool_filter = Some(vec![TOOL.to_owned()]);

    let done = turn(&state, id, chat, DRAWS).await;

    assert_eq!(done.requests.len(), 2, "{:?}", done.events);
    let first = &done.requests[0];
    assert_eq!(first["tool_choice"], "required");
    let names: Vec<&str> = first["tools"]
        .as_array()
        .expect("the image tool")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    assert_eq!(names, ["builtin:generate_image"]);
    assert!(offers_nothing(&done.requests[1]), "{}", done.requests[1]);
}

/// A dialect model's markup is parsed by the endpoint, so a reply that still
/// carries it is text: nothing here dispatches it as a call.
#[tokio::test]
async fn markup_through_the_endpoint_is_not_parsed_again() {
    let markup = format!(
        "<tool_call>\n{}\n</tool_call>",
        json!({ "name": TOOL, "arguments": {} })
    );
    let reply = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({ "choices": [{ "delta": { "content": markup } }] }),
        json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] }),
    );
    let (_dir, state) = state().await;
    let dialect = model(&state, |entry| {
        entry.tags = vec![FORMAT_QWEN_XML.to_owned()];
    })
    .await;

    let done = turn(&state, dialect, phone(&state, dialect).await, &reply).await;

    let called = done
        .events
        .iter()
        .any(|event| matches!(event, AgentEvent::ToolCallStart { .. }));
    assert!(!called, "{:?}", done.events);
    assert!(done.ended_well, "the markup was the answer");
}

/// The sentence the endpoint's loop guard ends a run with (ADR 0021): a
/// finished assistant message, `finish_reason: "stop"`, and no call.
const GUARD_ENDED: &str = "[gglib loop guard] The model repeated the same step after being told to change \
     approach, so gglib's loop guard stopped this run.";

/// When the endpoint's loop guard ends the run, it answers in the model's
/// place with a finished message. The run ends well, with that sentence as
/// its answer, not as a failure with nothing said.
#[tokio::test]
async fn the_endpoints_loop_guard_ending_the_run_ends_it_cleanly() {
    let reply = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({ "id": "chatcmpl-1", "object": "chat.completion.chunk", "choices": [
            { "index": 0, "delta": { "role": "assistant", "content": GUARD_ENDED } }
        ] }),
        json!({ "id": "chatcmpl-1", "object": "chat.completion.chunk", "choices": [
            { "index": 0, "delta": {}, "finish_reason": "stop" }
        ] }),
    );
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;

    let done = turn(&state, id, phone(&state, id).await, &reply).await;

    assert!(done.ended_well, "{:?}", done.events);
    let answer = done.events.iter().find_map(|event| match event {
        AgentEvent::FinalAnswer { content } => Some(content.clone()),
        _ => None,
    });
    assert_eq!(answer.as_deref(), Some(GUARD_ENDED), "{:?}", done.events);
}

/// A loop through this machine's endpoint leaves its guard's count to the
/// endpoint; one on a paired machine's model still counts its own here.
#[test]
fn only_a_loop_through_this_machines_endpoint_leaves_its_guard_uncounted() {
    let door = |name: Option<&str>| gglib_runtime::Door {
        root_url: "http://127.0.0.1:9931".to_owned(),
        key: Some("k".to_owned()),
        name: name.map(str::to_owned),
    };
    assert!(
        !super::reports_its_guard(&door(None)),
        "this machine's endpoint"
    );
    assert!(
        super::reports_its_guard(&door(Some("desk"))),
        "a paired machine's"
    );
}

/// ~60k characters of tool output, outside the protected tail.
fn conversation() -> Vec<AgentMessage> {
    let mut messages: Vec<_> = ["call_1", "call_2"]
        .map(|id| AgentMessage::Tool {
            tool_call_id: id.to_owned(),
            content: "x".repeat(30_000),
            images: Vec::new(),
        })
        .into();
    messages.extend((0..8).map(|_| AgentMessage::user("ok")));
    messages
}

/// A conversation over the model's context goes to this machine's endpoint
/// whole: the endpoint trims it to the context it serves, as it trims any
/// client's.
#[tokio::test]
async fn a_turn_through_the_endpoint_is_sent_whole_for_the_endpoint_to_trim() {
    let (_dir, state) = state().await;
    let id = model(&state, |entry| entry.context_length = Some(4_096)).await;
    let mut device = phone(&state, id).await;
    device.messages = conversation();

    let body = sent(&state, id, device).await;

    let oldest = body["messages"][0]["content"].as_str().map(str::len);
    assert_eq!(oldest, Some(30_000));
}
