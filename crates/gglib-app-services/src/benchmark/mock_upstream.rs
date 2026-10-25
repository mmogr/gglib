//! A stand-in llama-server for the proxy arm's tests. No model is loaded.
//!
//! Plain tokio over a socket, because this crate may not depend on `axum` or
//! `hyper` (`scripts/check_boundaries.sh`), and a dev-dependency counts. It
//! speaks just enough HTTP/1.1 for one request per connection, closing each
//! connection after its answer.
//!
//! It offers one tool, `read_lines`, and answers `POST /v1/chat/completions`
//! by what it is asked:
//!
//! - a conversation whose last message is a tool result gets a streamed final
//!   answer, so the agent loop can finish;
//! - `stream: false`, which only the proxy's repair re-issue sends, gets a
//!   call that fits the schema: `max_lines` the integer `42`;
//! - anything else gets a streamed call that breaks it: `max_lines` the string
//!   `"42"`, the way Llama 3.2 3B was measured breaking it
//!   (`docs/tool-call-repair.md`).
//!
//! Anything else gets a 404, and a connection that sends nothing, such as the
//! proxy's reachability probe, is closed. Every chat body is recorded.

use std::sync::{Arc, Mutex};

use gglib_core::domain::ModelCapabilities;
use gglib_core::ports::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};
use gglib_test_http::Incoming;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// The tool the mock's answers call.
pub(crate) const TOOL: &str = "read_lines";

/// Arguments that break the schema [`read_lines_task`] advertises.
pub(crate) const BROKEN_ARGS: &str = r#"{"path":"server.log","max_lines":"42"}"#;

/// Arguments that fit it.
pub(crate) const FIXED_ARGS: &str = r#"{"path":"server.log","max_lines":42}"#;

/// A running mock. Dropping it stops the server.
pub(crate) struct MockUpstream {
    /// The port it listens on, on `127.0.0.1`.
    pub(crate) port: u16,
    seen: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl MockUpstream {
    /// Bind a free loopback port and start answering.
    pub(crate) async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let task = tokio::spawn(async move {
            while let Ok(incoming) = gglib_test_http::incoming(&listener).await {
                tokio::spawn(answer(incoming, Arc::clone(&sink)));
            }
        });
        Self { port, seen, task }
    }

    /// The base URL an arm posts to when it bypasses the proxy.
    pub(crate) fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Every chat body received so far, in arrival order.
    pub(crate) fn seen(&self) -> Vec<Value> {
        self.seen.lock().expect("lock").clone()
    }
}

impl Drop for MockUpstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A catalog holding the one model the arm is pinned to: id 1, by this name.
///
/// It can call tools and has no dialect, so the proxy neither strips a
/// request's tools nor installs a grammar: what reaches the mock is what the
/// arm sent, plus whatever the proxy itself adds.
#[derive(Debug)]
pub(crate) struct OneModelCatalog(pub(crate) &'static str);

#[async_trait::async_trait]
impl ModelCatalogPort for OneModelCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(Vec::new())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok((name == "1" || name == self.0).then(|| ModelSummary {
            capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS,
            param_count: String::new(),
            ..ModelSummary::bare(1, self.0)
        }))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// The task the mock is built to answer: one call to [`TOOL`], whose
/// `max_lines` the schema says is an integer.
pub(crate) fn read_lines_task() -> gglib_core::domain::benchmark::tune::task::TuneTask {
    serde_json::from_value(json!({
        "id": "read_lines_integer",
        "category": "single_call",
        "user_prompt": "Show me the first forty-two lines of server.log.",
        "tools": [{
            "name": TOOL,
            "description": "Read the first lines of a file.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_lines": { "type": "integer" }
                },
                "required": ["path", "max_lines"],
                "additionalProperties": false
            }
        }],
        "expected": {
            "kind": "tool_calls",
            "calls": [{
                "name": TOOL,
                "required_args": { "path": "server.log", "max_lines": 42 }
            }]
        }
    }))
    .expect("task")
}

async fn answer(incoming: Incoming, seen: Arc<Mutex<Vec<Value>>>) {
    let Ok((request, mut stream)) = incoming.request().await else {
        return;
    };
    let request_line = request.line();
    if !request_line.starts_with("POST ") || !request_line.contains("/v1/chat/completions") {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .await;
        return;
    }
    let parsed: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
    seen.lock().expect("lock").push(parsed.clone());

    let after_tool = parsed["messages"]
        .as_array()
        .and_then(|m| m.last())
        .is_some_and(|m| m["role"] == "tool");
    let response = if after_tool {
        sse(&[
            json!({"choices": [{"index": 0, "delta": {"content": "Done."},
            "finish_reason": "stop"}]}),
        ])
    } else if parsed["stream"] == json!(false) {
        let body = json!({"choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "tool_calls": [{"id": "call_fixed", "type": "function",
                "function": {"name": TOOL, "arguments": FIXED_ARGS}}]
        }}]})
        .to_string();
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{body}",
            body.len()
        )
    } else {
        sse(&[
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0,
                "id": "call_broken", "type": "function",
                "function": {"name": TOOL, "arguments": BROKEN_ARGS}}]},
                "finish_reason": null}]}),
            json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        ])
    };
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// A streamed response of `chunks`, then `[DONE]`.
#[allow(
    clippy::format_push_string,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn sse(chunks: &[Value]) -> String {
    let mut out = String::from(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
    );
    for chunk in chunks {
        out.push_str(&format!("data: {chunk}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    out
}
