//! POST /api/agent/chat — server-side agentic loop with SSE streaming.
//!
//! The handler composes `LlmCompletionAdapter + McpToolExecutorAdapter +
//! AgentLoop::build` inline, spawns the loop as a background task, and bridges
//! the resulting `mpsc::Receiver<AgentEvent>` to an Axum [`Sse`] response.
//!
//! # Cancellation
//!
//! When the HTTP client disconnects (browser tab closed, `curl` killed, etc.),
//! Axum drops the SSE response and therefore the [`guard::AgentTaskGuard`] stream
//! wrapper. Its [`Drop`] impl calls [`JoinHandle::abort`], which cancels the
//! spawned `AgentLoop` task at its next `await` point — immediately stopping
//! LLM token generation and any in-flight tool calls without leaking compute
//! or resources.

mod dto;
mod guard;

pub use dto::{AgentChatRequest, AgentRequestConfig};

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use gglib_agent::AgentLoop;
use gglib_mcp::McpToolExecutorAdapter;
use gglib_runtime::LlmCompletionAdapter;
use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;
use gglib_core::domain::agent::{AgentConfig, AgentEvent};
use gglib_core::ports::{AgentError, LlmCompletionPort, ToolExecutorPort};
use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;

use guard::AgentTaskGuard;

/// `POST /api/agent/chat` — start an agentic conversation with SSE streaming.
///
/// # Request
///
/// ```json
/// {
///   "port": 9000,
///   "messages": [{"role": "user", "content": "What files are in src/?"}],
///   "config": null,
///   "tool_filter": null
/// }
/// ```
///
/// # Response
///
/// Content-Type: `text/event-stream`. Each frame carries one [`AgentEvent`]
/// serialised with `#[serde(tag = "type", rename_all = "snake_case")]`:
///
/// ```text
/// data: {"type":"text_delta","content":"Looking at the directory…"}
///
/// data: {"type":"tool_call_start","tool_call":{"id":"tc_1","name":"read_dir",…}}
///
/// data: {"type":"tool_call_complete","result":{"tool_call_id":"tc_1",…}}
///
/// data: {"type":"iteration_complete","iteration":1,"tool_calls":1}
///
/// data: {"type":"final_answer","content":"The src/ directory contains …"}
/// ```
///
/// # Cancellation
///
/// Closing the connection (e.g. `ctrl-C` in curl) aborts the background task
/// immediately — no further LLM tokens are generated and no further tools are
/// called.
pub async fn chat(
    State(state): State<AppState>,
    Json(req): Json<AgentChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    validate_port(&state, req.port).await?;

    let tool_filter: Option<HashSet<String>> =
        req.tool_filter.map(|f| f.into_iter().collect());
    let llm: Arc<dyn LlmCompletionPort> =
        Arc::new(LlmCompletionAdapter::with_client(req.port, state.http_client.clone(), None::<String>));
    let tool_executor: Arc<dyn ToolExecutorPort> =
        Arc::new(McpToolExecutorAdapter::new(state.mcp.clone()));
    let agent_loop = AgentLoop::build(llm, tool_executor, tool_filter);

    let messages = req.messages;
    let config: AgentConfig = req.config.unwrap_or_default().into();

    let (tx, rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let handle = tokio::spawn(async move {
        match agent_loop.run(messages, config, tx).await {
            Ok(_) => {}
            Err(e @ AgentError::Internal(_)) => {
                tracing::error!("agent loop failed with internal error: {e}");
            }
            Err(e) => tracing::warn!("agent loop ended: {e}"),
        }
    });

    let sse_stream = AgentTaskGuard {
        inner: ReceiverStream::new(rx),
        handle,
    }
    .filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                tracing::error!(error = %e, "agent: failed to serialise AgentEvent, dropping frame");
                None
            }
        })
    });

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    ))
}
