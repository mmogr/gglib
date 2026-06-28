//! `POST /api/benchmark/compare` — stream a side-by-side model comparison.
//!
//! Accepts a [`CompareConfig`] JSON body, spawns the comparison task in the
//! background, and returns an SSE stream of [`BenchmarkEvent`]s.
//!
//! # Cancellation
//!
//! When the HTTP client disconnects, Axum drops the SSE response and therefore
//! the [`BenchmarkTaskGuard`] stream wrapper.  Its [`Drop`] impl fires
//! `cancel.cancel()`, cooperatively signalling the benchmark task to stop at
//! the next model boundary — freeing VRAM and marking the run `Failed` in the
//! DB before exiting.  See [`gglib_app_services::benchmark::guard`] for details.

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
use gglib_core::domain::benchmark::{BenchmarkEvent, CompareConfig};

use crate::error::HttpError;
use crate::state::AppState;

/// `POST /api/benchmark/compare` — start a compare run and stream events.
///
/// # Request
///
/// ```json
/// {
///   "model_ids": [1, 2],
///   "prompt": "Explain gradient descent in one paragraph.",
///   "system_prompt": null,
///   "inference": null,
///   "ctx_size": null
/// }
/// ```
///
/// # Response
///
/// `Content-Type: text/event-stream`.  Each frame carries one [`BenchmarkEvent`]
/// serialised as JSON:
///
/// ```text
/// data: {"type":"model_started","model_id":1,"model_name":"llama-3b","position":1,"total":2}
///
/// data: {"type":"model_text_delta","model_id":1,"text":"Gradient descent is…"}
///
/// data: {"type":"model_complete","model_id":1,"result":{"kind":"compare",…}}
///
/// data: {"type":"run_complete","run_id":42}
/// ```
pub async fn compare_sse(
    State(state): State<AppState>,
    Json(config): Json<CompareConfig>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let cancel = CancellationToken::new();
    let (tx, rx) = mpsc::channel::<BenchmarkEvent>(256);

    let benchmark = state.benchmark.clone();
    let cancel_task = cancel.clone();

    tokio::spawn(async move {
        if let Err(e) = benchmark.run_compare(config, tx, cancel_task).await {
            tracing::error!(error = %e, "benchmark/compare: run failed");
        }
    });

    let guard = BenchmarkTaskGuard::new(ReceiverStream::new(rx), cancel);

    let sse_stream = guard.filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                tracing::error!(error = %e, "benchmark/compare: failed to serialise event");
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
