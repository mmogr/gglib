//! POST /api/agent/chat — server-side agentic loop with SSE streaming.
//!
//! The handler composes [`gglib_agent::AgentLoop`] with a
//! [`gglib_mcp::McpToolExecutorAdapter`] and a
//! [`gglib_runtime::LlmCompletionAdapter`], spawns the loop as a background
//! task, and bridges the resulting `mpsc::Receiver<AgentEvent>` to an Axum
//! [`Sse`] response.
//!
//! # Cancellation
//!
//! When the HTTP client disconnects (browser tab closed, `curl` killed, etc.),
//! Axum drops the SSE response and therefore the [`AgentTaskGuard`] stream
//! wrapper. Its [`Drop`] impl calls [`JoinHandle::abort`], which cancels the
//! spawned `AgentLoop` task at its next `await` point — immediately stopping
//! LLM token generation and any in-flight tool calls without leaking compute
//! or resources.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;
use gglib_agent::{AgentLoop, FilteredToolExecutor};
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_core::ports::{AgentLoopPort, ToolExecutorPort};
use gglib_mcp::McpToolExecutorAdapter;
use gglib_runtime::LlmCompletionAdapter;

// =============================================================================
// Request DTO
// =============================================================================

/// Request body for `POST /api/agent/chat`.
#[derive(Debug, Deserialize)]
pub struct AgentChatRequest {
    /// Port of the llama-server instance to drive.
    ///
    /// Must match a currently-running server (the same constraint as the chat
    /// proxy endpoint). Validated by [`validate_port`] before the loop starts.
    pub port: u16,

    /// Full conversation history in domain form.
    ///
    /// Supports all four [`AgentMessage`] variants: `system`, `user`,
    /// `assistant` (with or without `tool_calls`), and `tool`.
    pub messages: Vec<AgentMessage>,

    /// Optional loop configuration.
    ///
    /// When `None`, [`AgentConfig::default`] is used (matches the TypeScript
    /// frontend constants: `max_iterations = 25`, `tool_timeout_ms = 30 000`,
    /// etc.).
    pub config: Option<AgentConfig>,

    /// Optional allowlist of tool names to expose to the model.
    ///
    /// When `Some`, only tools whose names appear in this list are sent to the
    /// LLM and can be executed. When `None`, all tools from all connected MCP
    /// servers are available.
    pub tool_filter: Option<Vec<String>>,
}

// =============================================================================
// AgentTaskGuard — RAII cancellation guard
// =============================================================================

/// Wraps a [`ReceiverStream<AgentEvent>`] together with the [`JoinHandle`] of
/// the task that feeds it.
///
/// When this struct is dropped — either because the SSE stream reaches its
/// natural end **or** because the HTTP client disconnected and Axum dropped
/// the response — [`JoinHandle::abort`] is called immediately, cancelling the
/// background [`AgentLoop`] task at its next `await` point.
///
/// This prevents the loop from running to completion (burning tokens and CPU)
/// after the consumer has gone away.
struct AgentTaskGuard {
    inner: ReceiverStream<AgentEvent>,
    handle: JoinHandle<()>,
}

impl Drop for AgentTaskGuard {
    fn drop(&mut self) {
        // Idempotent: aborting an already-finished handle is a no-op.
        self.handle.abort();
    }
}

impl Stream for AgentTaskGuard {
    type Item = AgentEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // ReceiverStream is Unpin, so Pin::new is safe here.
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

// =============================================================================
// Handler
// =============================================================================

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

    // ── Compose the LLM adapter (shared reqwest::Client from AppState) ───
    let llm = Arc::new(LlmCompletionAdapter::with_client(
        req.port,
        state.http_client.clone(),
    ));

    // ── Compose the tool executor (MCP adapter, optionally filtered) ──────
    let mcp_executor = Arc::new(McpToolExecutorAdapter::new(Arc::clone(&state.mcp)));

    let tool_executor: Arc<dyn ToolExecutorPort> = match req.tool_filter {
        Some(filter) => Arc::new(FilteredToolExecutor::new(
            mcp_executor,
            filter.into_iter().collect(),
        )),
        None => mcp_executor,
    };

    // ── Build the AgentLoop (stateless, cheap to construct) ───────────────
    let agent_loop = AgentLoop::new(llm, tool_executor);
    let messages = req.messages;
    let config = req.config.unwrap_or_default();

    // ── Pipe AgentEvent values from the loop to the SSE stream ───────────
    let (tx, rx) = mpsc::channel::<AgentEvent>(256);

    let handle = tokio::spawn(async move {
        match agent_loop.run(messages, config, tx).await {
            Ok(_) => {} // history return value not needed for stateless HTTP handler
            Err(e) => tracing::warn!("agent loop ended with error: {e}"),
        }
    });

    // AgentTaskGuard ensures handle.abort() is called when the SSE stream is dropped.
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
