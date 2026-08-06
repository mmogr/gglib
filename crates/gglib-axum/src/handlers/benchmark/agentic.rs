//! `POST /api/benchmark/agentic` — stream a raw-vs-gglib A/B agentic eval.
//!
//! Accepts an [`AgenticEvalConfig`] JSON body, spawns the eval in the
//! background, and returns an SSE stream of [`BenchmarkEvent`]s ending in
//! `agentic_eval_complete` with the full A/B report.
//!
//! Same [`BenchmarkTaskGuard`] `Drop`-cancels pattern and payload-limit
//! considerations as `tune` — a custom `task_suite` can be large, so the
//! route registration applies the same 5 MiB body limit.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use gglib_app_services::benchmark::guard::BenchmarkTaskGuard;
use gglib_core::domain::benchmark::{AgenticEvalConfig, BenchmarkEvent};

use crate::error::HttpError;
use crate::state::AppState;

/// `POST /api/benchmark/agentic` — start an A/B eval and stream events.
///
/// # Request
///
/// ```json
/// {
///   "model_id": 1,
///   "task_suite": { "source": "default" },
///   "weights": { "tool_accuracy": 0.4, "loop_avoidance": 0.3, "task_completion": 0.2, "speed": 0.1 },
///   "ctx_size": null
/// }
/// ```
///
/// # Response
///
/// `Content-Type: text/event-stream`. Frames carry [`BenchmarkEvent`]s
/// (`agentic_arm_started`, `agentic_task_complete`,
/// `agentic_eval_complete`, `run_failed`).
pub async fn agentic_sse(
    State(state): State<AppState>,
    Json(config): Json<AgenticEvalConfig>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let cancel = CancellationToken::new();
    let (tx, rx) = mpsc::channel::<BenchmarkEvent>(256);

    let benchmark = state.benchmark.clone();
    let cancel_task = cancel.clone();

    tokio::spawn(async move {
        if let Err(e) = benchmark.run_agentic(config, tx, cancel_task).await {
            tracing::error!(error = %e, "benchmark/agentic: run failed");
        }
    });

    let guard = BenchmarkTaskGuard::new(ReceiverStream::new(rx), cancel);

    let sse_stream = guard.filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                tracing::error!(error = %e, "benchmark/agentic: failed to serialise event");
                None
            }
        })
    });

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}
