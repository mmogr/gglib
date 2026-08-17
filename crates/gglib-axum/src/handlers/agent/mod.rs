#![doc = include_str!("README.md")]
mod dto;
mod guard;
mod retry_notice;

pub(crate) use dto::AgentChatRequest;

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

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;
use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent};
use gglib_core::ports::{AgentError, RetryObserver};
use gglib_core::request_pipeline;
use gglib_runtime::compose_agent_loop;

use guard::AgentTaskGuard;
use retry_notice::RetryNotice;

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
pub(crate) async fn chat(
    State(state): State<AppState>,
    Json(req): Json<AgentChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    // Acquire a concurrency permit — reject immediately with 429 if all
    // slots are occupied rather than queuing (each active agent loop
    // consumes LLM inference time and tool I/O).
    let permit = state
        .agent_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            HttpError::TooManyRequests("all agent loop slots are in use; try again later".into())
        })?;

    validate_port(&state, req.port).await?;

    // Read before `tool_filter` consumes the request piecemeal, and before the
    // loop is composed: the two reasoning controls are the only sampling this
    // endpoint accepts, and they occupy the ladder's top rung.
    let sampling = req.sampling_layer();
    let tool_filter: Option<HashSet<String>> = req.tool_filter.map(|f| f.into_iter().collect());
    let model_context =
        request_pipeline::resolve(state.catalog.as_ref(), req.model.as_deref()).await;

    // Created before the loop is composed so the completion adapter can report
    // its retries onto the same stream the loop emits through — otherwise a
    // contended model is indistinguishable from a hung one for as long as the
    // retry budget lasts.
    let (tx, rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
    let retry_observer: Arc<dyn RetryObserver> = Arc::new(RetryNotice::new(tx.clone()));

    let agent_loop = compose_agent_loop(
        format!("http://127.0.0.1:{}", req.port),
        state.http_client.clone(),
        req.model.clone(),
        model_context,
        state.mcp.clone(),
        tool_filter,
        // GUI chat runs in the same process as the embedded proxy; report its
        // reuse to the shared agent-path store behind `agent_usage`.
        Some(state.proxy.agent_metrics()),
        Some(retry_observer),
        sampling,
    );

    let messages = req.messages;
    // Stagnation threshold is a persisted server-side setting, not a request
    // field; a settings-read failure falls back to the built-in default.
    let max_stagnation_steps = state
        .settings
        .get()
        .await
        .ok()
        .and_then(|s| s.max_stagnation_steps)
        .map(|v| v as usize);
    let config: AgentConfig = req
        .config
        .unwrap_or_default()
        .into_agent_config(max_stagnation_steps);

    // Move the semaphore permit into the spawned task so it is held for the
    // full duration of the agent loop.  When the task completes (or is
    // aborted by AgentTaskGuard on client disconnect), the permit is dropped
    // and the slot becomes available for new requests.
    let handle = tokio::spawn(async move {
        let _permit = permit;
        match agent_loop.run(messages, config, tx).await {
            Ok(output) => {
                tracing::debug!(
                    total_iterations = output.total_iterations,
                    "agent loop completed"
                );
            }
            Err(e @ AgentError::Internal(_)) => {
                tracing::error!("agent loop failed with internal error: {e}");
            }
            Err(e) => tracing::warn!("agent loop ended: {e}"),
        }
    });

    let sse_stream = AgentTaskGuard::new(ReceiverStream::new(rx), handle)
        .filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                // Silently dropping a frame here would leave the client hanging
                // indefinitely — especially fatal if the failed event is
                // `FinalAnswer` or `Error`. Construct a typed fallback event so
                // the client always receives a terminal signal that is
                // structurally valid regardless of future AgentEvent changes.
                tracing::error!(error = %e, "agent: failed to serialise AgentEvent; emitting fallback error");
                let typed_fallback = AgentEvent::Error {
                    message: "serialization failed".to_owned(),
                };
                let fallback = serde_json::to_string(&typed_fallback)
                    .unwrap_or_else(|_| r#"{"type":"error","message":"serialization failed"}"#.to_owned());
                Some(Ok::<Event, Infallible>(Event::default().data(fallback)))
            }
        })
    });

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    ))
}
