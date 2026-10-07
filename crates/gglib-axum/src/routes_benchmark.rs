//! The benchmark's routes, nested under `/api/benchmark`.
//!
//! Moved out of `routes.rs`, which is at its size budget, as `/remote` was.

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use crate::handlers;
use crate::state::AppState;

/// Every `/api/benchmark/*` route. Nested by the caller, so the paths here are
/// relative.
pub(crate) fn benchmark_routes() -> Router<AppState> {
    Router::new()
        // Benchmark — compare and perf SSE streams
        .route("/compare", post(handlers::benchmark::compare::compare_sse))
        .route("/perf", post(handlers::benchmark::perf::perf_sse))
        // Benchmark — tune SSE stream (sampling-parameter sweep)
        //
        // Body limit: **5 MiB** (vs the Axum default of 2 MiB).
        //
        // A custom `task_suite` can embed `long_context` tasks with thousands
        // of tokens of simulated prior-session history per task, so the
        // default limit is comfortably breached by a handful of scenarios.
        .route(
            "/tune",
            post(handlers::benchmark::tune::tune_sse).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        // Benchmark — gated apply of a completed tune run's winner. Plain
        // JSON, not SSE: the gate judges stored results, no model runs.
        .route(
            "/tune/{run_id}/apply",
            post(handlers::benchmark::tune::tune_apply),
        )
        // Benchmark — raw-vs-gglib A/B agentic eval SSE stream. Same body
        // limit as tune: a custom task_suite can embed long_context tasks.
        .route(
            "/agentic",
            post(handlers::benchmark::agentic::agentic_sse)
                .layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        // Benchmark — run history
        .route("/runs", get(handlers::benchmark::history::list_runs))
}
