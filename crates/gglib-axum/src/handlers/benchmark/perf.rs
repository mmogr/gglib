//! `POST /api/benchmark/perf` — stream a `llama-bench` performance run.
//!
//! Accepts a [`PerfConfig`] JSON body, spawns the perf task in the background,
//! and returns an SSE stream of [`BenchmarkEvent`]s.
//!
//! # Cancellation
//!
//! Identical to the compare handler: dropping the SSE response fires the
//! [`BenchmarkTaskGuard`]'s `cancel.cancel()`, letting the task stop VRAM
//! cleanly between models.

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
use gglib_core::domain::benchmark::{BenchmarkEvent, PerfConfig};

use crate::error::HttpError;
use crate::state::AppState;

/// `POST /api/benchmark/perf` — start a perf run and stream events.
///
/// # Request
///
/// ```json
/// {
///   "model_ids": [1],
///   "pp_tokens": 512,
///   "tg_tokens": 128,
///   "repetitions": 3
/// }
/// ```
///
/// # Response
///
/// `Content-Type: text/event-stream`.  Each frame carries one [`BenchmarkEvent`]:
///
/// ```text
/// data: {"type":"model_started","model_id":1,"model_name":"llama-3b","position":1,"total":1}
///
/// data: {"type":"model_complete","model_id":1,"result":{"kind":"perf","tg_tps":52.3,…}}
///
/// data: {"type":"run_complete","run_id":17}
/// ```
pub async fn perf_sse(
    State(state): State<AppState>,
    Json(config): Json<PerfConfig>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let cancel = CancellationToken::new();
    let (tx, rx) = mpsc::channel::<BenchmarkEvent>(64);

    let benchmark = state.benchmark.clone();
    let cancel_task = cancel.clone();

    tokio::spawn(async move {
        if let Err(e) = benchmark.run_perf(config, tx, cancel_task).await {
            tracing::error!(error = %e, "benchmark/perf: run failed");
        }
    });

    let guard = BenchmarkTaskGuard::new(ReceiverStream::new(rx), cancel);

    let sse_stream = guard.filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                tracing::error!(error = %e, "benchmark/perf: failed to serialise event");
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
