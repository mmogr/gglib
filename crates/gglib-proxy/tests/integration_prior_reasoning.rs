//! End-to-end tests for stage 1 through the proxy: earlier assistant turns'
//! reasoning reaches llama-server only where the model's template keeps it
//! (ADR 0020).
//!
//! The pipeline reads the caps from the catalog row, where the runtime records
//! a launch's `GET /props` reading; the proxy's own `/props` read feeds only
//! the dashboard. So the fixture sets the caps on the row, and the assertions
//! read the body the upstream received.

use gglib_core::domain::TemplateCaps;
use serde_json::{Value, json};

mod fixtures;
use fixtures::profile_harness::spawn_with_template_caps;
use fixtures::profile_mocks::MODEL;

/// What Copilot CLI sends on a later turn: its earlier assistant message with
/// the `reasoning_content` it received, and a tool result after it.
fn replayed_history() -> Value {
    json!({
        "model": MODEL,
        "stream": false,
        "messages": [
            {"role": "user", "content": "plan it"},
            {"role": "assistant", "content": "<think>inline</think>reading",
             "reasoning_content": "the earlier trace",
             "tool_calls": [{"id": "c1", "type": "function",
                             "function": {"name": "read", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "c1", "content": "file"},
        ],
    })
}

fn caps(preserve: bool) -> TemplateCaps {
    TemplateCaps {
        supports_preserve_reasoning: Some(preserve),
        ..TemplateCaps::default()
    }
}

#[tokio::test]
async fn a_template_that_preserves_reasoning_is_sent_it() {
    let harness = spawn_with_template_caps(Some(caps(true))).await;
    let sent = replayed_history();
    assert!(harness.post(sent.clone()).await.status().is_success());

    let forwarded = harness.only_forwarded();
    assert_eq!(forwarded["messages"], sent["messages"], "{forwarded}");
}

#[tokio::test]
async fn a_template_that_does_not_preserve_reasoning_is_not() {
    let harness = spawn_with_template_caps(Some(caps(false))).await;
    assert!(harness.post(replayed_history()).await.status().is_success());

    let assistant = &harness.only_forwarded()["messages"][1];
    assert!(assistant.get("reasoning_content").is_none(), "{assistant}");
    assert_eq!(assistant["content"], "reading");
}

/// No caps on the row: the model was never launched, or its build reported
/// none. Unknown strips, as before.
#[tokio::test]
async fn unknown_caps_strip_as_before() {
    let harness = spawn_with_template_caps(None).await;
    assert!(harness.post(replayed_history()).await.status().is_success());

    let assistant = &harness.only_forwarded()["messages"][1];
    assert!(assistant.get("reasoning_content").is_none(), "{assistant}");
}
