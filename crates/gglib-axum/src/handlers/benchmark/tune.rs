//! `POST /api/benchmark/tune` — stream a sampling-parameter sweep run.
//!
//! Accepts a [`TuneConfig`] JSON body, spawns the tune task in the
//! background, and returns an SSE stream of [`BenchmarkEvent`]s.
//!
//! # Payload size
//!
//! Unlike `compare`/`perf`, a tune request's `task_suite` field may carry a
//! user-authored [`TaskSuite::Custom`](gglib_core::domain::benchmark::tune::task::TaskSuite::Custom)
//! with `long_context` tasks embedding thousands of tokens of simulated
//! history. The route registration in `routes.rs` applies an explicit
//! `DefaultBodyLimit` override (5 MiB, vs Axum's 2 MiB default) so these
//! payloads are never rejected before the handler even runs.
//!
//! # Cancellation
//!
//! Same [`BenchmarkTaskGuard`] `Drop`-cancels pattern as `compare`/`perf`:
//! client disconnect cancels the task cooperatively at the next candidate
//! boundary, marking the run `Failed` in the DB.

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
use gglib_core::domain::benchmark::BenchmarkEvent;
use gglib_core::domain::benchmark::tune::config::TuneConfig;

use crate::error::HttpError;
use crate::state::AppState;

/// `POST /api/benchmark/tune` — start a tune run and stream events.
///
/// # Request
///
/// ```json
/// {
///   "model_id": 1,
///   "task_suite": { "source": "default" },
///   "sweep": { "temperature": [0.2, 0.5, 0.8], "top_p": [0.9, 0.95], "top_k": [], "min_p": [], "repeat_penalty": [] },
///   "seed_from_gguf": true,
///   "seed_from_family_presets": true,
///   "weights": { "tool_accuracy": 0.4, "loop_avoidance": 0.3, "task_completion": 0.2, "speed": 0.1 },
///   "prune_fraction": 0.5,
///   "ctx_size": null
/// }
/// ```
///
/// A custom suite is sent the same way the CLI parses one from a file —
/// `"task_suite": { "source": "custom", "tasks": [ ... ] }` — there is one
/// shared [`TaskSuite`](gglib_core::domain::benchmark::tune::task::TaskSuite)
/// schema for both adapters.
///
/// # Response
///
/// `Content-Type: text/event-stream`. Each frame carries one [`BenchmarkEvent`]
/// serialised as JSON (`tune_candidate_started`, `tune_task_complete`,
/// `tune_pruned`, `tune_candidate_complete`, `run_complete`/`run_failed`).
pub async fn tune_sse(
    State(state): State<AppState>,
    Json(config): Json<TuneConfig>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let cancel = CancellationToken::new();
    let (tx, rx) = mpsc::channel::<BenchmarkEvent>(256);

    let benchmark = state.benchmark.clone();
    let cancel_task = cancel.clone();

    tokio::spawn(async move {
        if let Err(e) = benchmark.run_tune(config, tx, cancel_task).await {
            tracing::error!(error = %e, "benchmark/tune: run failed");
        }
    });

    let guard = BenchmarkTaskGuard::new(ReceiverStream::new(rx), cancel);

    let sse_stream = guard.filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                tracing::error!(error = %e, "benchmark/tune: failed to serialise event");
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
