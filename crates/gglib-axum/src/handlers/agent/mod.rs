#![doc = include_str!("README.md")]
mod compose;
mod dto;
mod guard;
mod hub_model;
pub(crate) mod hub_turn;
mod image_gate;
mod launch;
mod remote_upstream;
mod retry_notice;
mod run;
mod transcript;

pub(crate) use dto::AgentChatRequest;
pub(crate) use run::create_run;

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::domain::agent::AgentEvent;
use gglib_core::ports::AgentError;

use compose::{Prepared, frame, prepare, refuse_unavailable_drawing, take_permit};
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
pub(crate) async fn chat(
    State(state): State<AppState>,
    Json(req): Json<AgentChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    // Before a slot is taken: a message sent with Draw pressed that this
    // machine cannot draw for is refused, saying why.
    refuse_unavailable_drawing(&state, &req).await?;
    // Acquire a concurrency permit — reject immediately with 429 if all
    // slots are occupied rather than queuing (each active agent loop
    // consumes LLM inference time and tool I/O).
    let permit = take_permit(&state).ok_or_else(|| {
        HttpError::TooManyRequests("all agent loop slots are in use; try again later".into())
    })?;

    let Prepared {
        agent_loop,
        messages,
        config,
        tx,
        rx,
        ..
    } = prepare(&state, req).await?;

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
        .filter_map(|event| std::future::ready(sse_event(&event).map(Ok::<Event, Infallible>)));

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    ))
}

/// One event as this route's SSE frame; none for a tool's preview frame,
/// which this route does not carry (a run's readers get it beside the log).
fn sse_event(event: &AgentEvent) -> Option<Event> {
    if matches!(event, AgentEvent::ToolPreview { .. }) {
        return None;
    }
    Some(Event::default().data(frame(event)))
}

#[cfg(test)]
#[path = "run_answer_tests.rs"]
mod run_answer_tests;
#[cfg(test)]
#[path = "run_busy_tests.rs"]
mod run_busy_tests;
#[cfg(test)]
#[path = "run_end_tests.rs"]
mod run_end_tests;
#[cfg(test)]
pub(in crate::handlers) mod run_fixture;
#[cfg(test)]
#[path = "run_hold_tests.rs"]
mod run_hold_tests;
#[cfg(test)]
#[path = "run_machine_tests.rs"]
mod run_machine_tests;
#[cfg(test)]
#[path = "run_made_tests.rs"]
mod run_made_tests;
#[cfg(test)]
#[path = "run_model_tests.rs"]
mod run_model_tests;
#[cfg(test)]
#[path = "run_preview_tests.rs"]
mod run_preview_tests;
#[cfg(test)]
#[path = "run_privacy_tests.rs"]
mod run_privacy_tests;
#[cfg(test)]
#[path = "run_rows_tests.rs"]
mod run_rows_tests;
#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;
#[cfg(test)]
#[path = "run_thinking_tests.rs"]
mod run_thinking_tests;
#[cfg(test)]
#[path = "transcript_images_tests.rs"]
mod transcript_images_tests;
#[cfg(test)]
mod turn_fixture;
