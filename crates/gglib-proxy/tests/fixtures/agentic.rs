//! A whole proxy in front of one model that can call tools, and the body
//! its upstream is handed: for tests of what the proxy does to a request
//! that carries tools.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::common::{FixedUpstream, spawn_proxy_with_settings};
use super::loop_guard::spawn_recording_upstream;
use gglib_core::Settings;
use gglib_core::domain::ModelCapabilities;
use gglib_core::ports::{
    CatalogError, InMemorySettings, ModelCatalogPort, ModelLaunchSpec, ModelRuntimePort,
    ModelSummary,
};

pub(crate) const MODEL: &str = "tool-model";

/// A catalogue of one model that can call tools, so the pipeline keeps the
/// request's `tools` and the turn is an agentic one.
#[derive(Debug)]
pub(crate) struct ToolCallingCatalog;

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
pub(crate) fn tool_request(the_user_spoke_last: bool) -> Value {
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
pub(crate) async fn sent_upstream(settings: Settings, request: Value) -> (Value, Value) {
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
    (body, status)
}
