//! The proxy's thinking policy, read off the body the upstream is handed.
//!
//! A request that carries tools, sent through a whole proxy to a model that
//! can call them: what `reasoning_budget_tokens` the upstream receives, by who
//! spoke last and by what the settings store.

mod fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use fixtures::common::{FixedUpstream, spawn_proxy_with_settings};
use fixtures::loop_guard::{assistant_call, looping_history, spawn_recording_upstream};
use gglib_core::domain::ModelCapabilities;
use gglib_core::ports::{
    CatalogError, InMemorySettings, ModelCatalogPort, ModelLaunchSpec, ModelRuntimePort,
    ModelSummary,
};
use gglib_core::{AgenticThinking, Settings};

const MODEL: &str = "tool-model";

/// A catalogue of one model that can call tools, so the pipeline keeps the
/// request's `tools` and the turn is an agentic one.
#[derive(Debug)]
struct ToolCallingCatalog;

impl ToolCallingCatalog {
    fn summary() -> ModelSummary {
        ModelSummary {
            capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS
                | ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
            ..ModelSummary::bare(1, MODEL)
        }
    }
}

#[async_trait]
impl ModelCatalogPort for ToolCallingCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![Self::summary()])
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok((name == MODEL).then(Self::summary))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// A request with one tool whose history ends in a tool result, or, with
/// `the_user_spoke_last`, in a message from the user after it.
fn tool_request(the_user_spoke_last: bool) -> Value {
    let mut messages = vec![
        json!({ "role": "system", "content": "be helpful" }),
        json!({ "role": "user", "content": "read the file" }),
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "c1",
                "type": "function",
                "function": { "name": "read_file", "arguments": "{\"path\":\"a.rs\"}" }
            }]
        }),
        json!({ "role": "tool", "tool_call_id": "c1", "content": "fn main() {}" }),
    ];
    if the_user_spoke_last {
        messages.push(json!({ "role": "user", "content": "now explain it" }));
    }
    json!({
        "model": MODEL,
        "stream": true,
        "messages": messages,
        "tools": [{
            "type": "function",
            "function": {
                "name": "read_file",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" } } }
            }
        }],
    })
}

/// Send `request` through a proxy running under `settings`, and return the
/// body its upstream was handed and the proxy's status afterwards.
async fn forwarded(settings: Settings, request: Value) -> (Value, Value) {
    let upstream_cancel = CancellationToken::new();
    let (upstream_port, seen) = spawn_recording_upstream(upstream_cancel.clone()).await;
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: MODEL.into(),
        slot_restore_supported: true,
        pinned: false,
    });
    let (proxy_url, proxy_cancel) = spawn_proxy_with_settings(
        runtime,
        Arc::new(ToolCallingCatalog),
        Arc::new(InMemorySettings::with(settings)),
    )
    .await;

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&request)
        .send()
        .await
        .expect("proxy request");
    assert_eq!(resp.status().as_u16(), 200);
    // Drain the stream so the forward completes before the status is read.
    let _ = resp.bytes().await;

    let status: Value = Client::new()
        .get(format!("{proxy_url}/v1/proxy/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");

    let body = seen
        .lock()
        .unwrap()
        .clone()
        .expect("the upstream was called");
    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let body: Value = serde_json::from_slice(&body).expect("the forwarded body is JSON");
    // Every case here is about a turn that still carries its tools: without
    // them no policy applies, and an absent budget would prove nothing.
    assert!(
        body["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty()),
        "{body}"
    );
    (body, status)
}

fn storing(policy: Option<AgenticThinking>) -> Settings {
    Settings {
        agentic_thinking: policy,
        ..Settings::with_defaults()
    }
}

/// The default, with nothing stored: a turn that follows a tool result is
/// sent a budget of 0, and the same conversation with the user's message
/// after it is sent none.
#[tokio::test]
async fn an_unset_install_stops_thinking_after_a_tool_result_and_not_after_the_user() {
    for stored in [None, Some(AgenticThinking::WhenAsked)] {
        let (after_tool, status) = forwarded(storing(stored), tool_request(false)).await;
        assert_eq!(
            after_tool["reasoning_budget_tokens"],
            json!(0),
            "{stored:?}"
        );
        assert!(after_tool.get("reasoning_budget_message").is_none());
        // The proxy's own record of the turn names the policy, not a rung.
        let recorded = &status["sampling_audit"]["reasoning"]["latest"]["budget"];
        assert_eq!(recorded["tokens"], json!(0), "{status}");
        assert_eq!(recorded["source"], json!("agentic turn"), "{status}");

        let (after_user, status) = forwarded(storing(stored), tool_request(true)).await;
        assert!(
            after_user.get("reasoning_budget_tokens").is_none(),
            "{stored:?}: {after_user}"
        );
        let latest = &status["sampling_audit"]["reasoning"]["latest"];
        assert!(latest.is_object(), "the turn was recorded: {status}");
        assert!(latest["budget"].is_null(), "{status}");
    }
}

#[tokio::test]
async fn each_stored_policy_reaches_the_upstream_as_its_own_budget() {
    let (off, _) = forwarded(storing(Some(AgenticThinking::Off)), tool_request(true)).await;
    assert_eq!(off["reasoning_budget_tokens"], json!(0));
    assert!(off.get("reasoning_budget_message").is_none());

    let (brief, _) = forwarded(storing(Some(AgenticThinking::Brief)), tool_request(false)).await;
    assert_eq!(brief["reasoning_budget_tokens"], json!(512));
    assert_eq!(
        brief["reasoning_budget_message"],
        json!("Enough thinking. Act now.")
    );

    let (unchanged, _) =
        forwarded(storing(Some(AgenticThinking::Default)), tool_request(false)).await;
    assert!(
        unchanged.get("reasoning_budget_tokens").is_none(),
        "{unchanged}"
    );
}

/// The way out for a client: its own budget, `-1` for the model's default.
#[tokio::test]
async fn a_clients_own_budget_reaches_the_upstream_as_sent() {
    for sent in [-1, 0, 2048] {
        let mut request = tool_request(false);
        request["reasoning_budget_tokens"] = json!(sent);
        let (body, _) = forwarded(storing(None), request).await;

        assert_eq!(body["reasoning_budget_tokens"], json!(sent));
    }
}

/// `agentic_sampling` stored off switches the policy off with the ceiling.
#[tokio::test]
async fn the_agentic_sampling_switch_turns_the_policy_off() {
    let settings = Settings {
        agentic_sampling: Some(false),
        agentic_thinking: Some(AgenticThinking::Off),
        ..Settings::with_defaults()
    };
    let (body, _) = forwarded(settings, tool_request(false)).await;

    assert!(body.get("reasoning_budget_tokens").is_none(), "{body}");
}

/// The loop guard's note goes on as a new `user` message when the request's
/// last message has no text to add it to. The person did not write it, so
/// `when-asked` reads who spoke last in what the client sent, and the turn is
/// still sent a budget of 0.
#[tokio::test]
async fn the_loop_guards_own_user_message_is_not_the_person_speaking() {
    let mut request = tool_request(false);
    let mut messages = vec![json!({ "role": "user", "content": "fix it" })];
    messages.extend(looping_history(3));
    // The fourth call of the batch, with no text and no result yet.
    messages.push(assistant_call("write_file", r#"{"path":"src/main.rs"}"#));
    request["messages"] = json!(messages);

    let (body, status) = forwarded(storing(None), request).await;

    let last = body["messages"].as_array().unwrap().last().unwrap();
    assert_eq!(last["role"], json!("user"), "{body}");
    let note = last["content"].as_str().expect("the note is text");
    assert!(note.contains("[gglib loop guard]"), "{note}");
    assert_eq!(
        status["recent_requests"][0]["loop_guard_trip"],
        json!("loop")
    );
    assert_eq!(body["reasoning_budget_tokens"], json!(0), "{body}");
}
