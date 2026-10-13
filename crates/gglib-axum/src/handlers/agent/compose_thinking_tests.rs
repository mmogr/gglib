//! The proxy's `agentic_thinking` policy and a daemon turn (ADR 0019,
//! decision 6): a turn on this machine's model is sent no budget whatever
//! policy is stored, and a turn on a paired machine's model says its own
//! budget, so that machine's proxy sets none.

use gglib_core::AgenticThinking;
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::settings::SettingsUpdate;
use serde_json::{Value, json};

use super::{Prepared, prepare_over};
use crate::handlers::agent::AgentChatRequest;
use crate::handlers::agent::remote_upstream::remote;
use crate::handlers::agent::run_fixture::state;
use crate::handlers::agent::turn_fixture::{REPLY, TOOL, carries_tools, doors, model, sent};
use crate::handlers::remote::fake_far::{FINGERPRINT, far as fake_far};
use crate::state::AppState;

const KEY: &str = "reasoning_budget_tokens";

async fn store(state: &AppState, policy: AgenticThinking) {
    let update = SettingsUpdate {
        agentic_thinking: Some(Some(policy)),
        ..SettingsUpdate::default()
    };
    state.core.settings().update(update).await.unwrap();
}

/// Each policy that sets a budget on the proxy sets none here: the request
/// carries tools and reaches the model with no budget, from the page and
/// from a paired device.
#[tokio::test]
async fn a_stored_policy_sets_no_budget_on_a_tool_turn_for_this_machines_model() {
    for policy in [
        AgenticThinking::Off,
        AgenticThinking::Brief,
        AgenticThinking::WhenAsked,
    ] {
        let (_dir, state) = state().await;
        let id = model(&state, |_| {}).await;
        store(&state, policy).await;
        let stored = state.core.settings().get().await.unwrap();
        assert_eq!(stored.agentic_thinking, Some(policy));

        for (door, chat) in doors(&state, id, true).await {
            let body = sent(&state, id, chat).await;
            assert!(carries_tools(&body), "{door}, {policy:?}");
            assert!(body.get(KEY).is_none(), "{door}, {policy:?}: {body}");
            assert!(
                body.get("reasoning_budget_message").is_none(),
                "{door}, {policy:?}: {body}"
            );
        }
    }
}

/// The far detail route's answer for model 3, as a gglib proxy writes it.
const LOOKUP: &str = r#"{"detail":{"id":3,"name":"org/qwen3","paramCountB":8.0,
    "quantization":"Q4_K_M","addedAt":"2026-10-01 09:00:00","isServing":false,"metadata":{}}}"#;

/// The body the paired machine's proxy is sent for a turn with one tool,
/// and with `more` beside it in the run's request.
async fn sent_far(more: &str) -> Value {
    let (_dir, state) = state().await;
    let (fake, far) = fake_far(200, LOOKUP).await;
    let there = ModelRef {
        machine: Machine::Paired {
            fingerprint: FINGERPRINT.to_owned(),
        },
        id: 3,
    };
    let upstream = remote(&far, &there).await.expect("looked up");
    *fake.body.lock().unwrap() = REPLY.to_owned();
    *fake.content_type.lock().unwrap() = "text/event-stream";

    let hi = r#"{"role":"user","content":"hi"}"#;
    let chat = format!(r#"{{"port":0,"messages":[{hi}],"tool_filter":["{TOOL}"]{more}}}"#);
    let chat: AgentChatRequest = serde_json::from_str(&chat).expect("parses");
    let Prepared {
        agent_loop,
        messages,
        config,
        tx,
        rx: _rx,
        ..
    } = prepare_over(&state, chat, upstream).await;
    let ended = agent_loop.run(messages, config, tx).await;
    assert!(ended.is_ok(), "{:?}", ended.err());

    let turn = fake.seen.lock().unwrap().last().expect("a request").clone();
    assert_eq!(turn.uri, "/v1/chat/completions");
    let body: Value = serde_json::from_str(&turn.body).unwrap();
    assert!(carries_tools(&body), "{body}");
    body
}

/// With Thinking on the run names no budget, and that machine's proxy would
/// set its own: the request asks for the model's launch default.
#[tokio::test]
async fn a_far_tool_turn_with_thinking_on_asks_for_the_launch_default() {
    let body = sent_far("").await;

    assert_eq!(body[KEY], json!(-1), "{body}");
}

/// Thinking off is still a budget of `0`.
#[tokio::test]
async fn a_far_tool_turn_with_thinking_off_sends_a_budget_of_zero() {
    let body = sent_far(r#","reasoning_budget_tokens":0"#).await;

    assert_eq!(body[KEY], json!(0), "{body}");
}
