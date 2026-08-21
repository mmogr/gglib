//! Daemon API route constants — the paths the CLI sends to `gglib daemon`.
//!
//! These live here, in shared vocabulary, rather than inside the CLI, so the
//! daemon's own test suite can walk them and fail when it stops serving one.
//! #834 deleted a route the CLI's download poller was calling and the whole
//! suite stayed green, because nothing tied the client's paths to the router's.
//! `gglib-axum/tests/daemon_route_contract.rs` is what ties them now.

/// Daemon identity probe.
pub const HEALTH_PATH: &str = "/health";

/// Start the proxy.
pub const PROXY_START_PATH: &str = "/api/proxy/start";

/// Stop the proxy.
pub const PROXY_STOP_PATH: &str = "/api/proxy/stop";

/// Current proxy status.
pub const PROXY_STATUS_PATH: &str = "/api/proxy/status";

/// Start (or reuse) a llama-server for a model.
pub const SERVERS_START_PATH: &str = "/api/servers/start";

/// Ask the daemon to shut down.
pub const DAEMON_SHUTDOWN_PATH: &str = "/api/daemon/shutdown";

/// Download queue: `POST` enqueues, `GET` returns the snapshot.
///
/// One path for both verbs. The snapshot handler was once double-mounted at
/// `/api/models/downloads` as well; when that mount was retired the CLI was
/// still polling it, and the bare path fell through to `/api/models/{id}`,
/// whose `i64` extractor answers `400 text/plain`.
pub const DOWNLOADS_QUEUE_PATH: &str = "/api/models/downloads/queue";

/// Model list. `gglib model list` reaches this on the *detected* daemon port
/// rather than the compile-time one, so it builds its own base — the path is
/// still the daemon's.
pub const MODELS_LIST_PATH: &str = "/api/models";

/// Benchmark comparison run (SSE).
pub const BENCHMARK_COMPARE_PATH: &str = "/api/benchmark/compare";

/// Benchmark performance run (SSE).
pub const BENCHMARK_PERF_PATH: &str = "/api/benchmark/perf";

/// Benchmark tuning run (SSE).
pub const BENCHMARK_TUNE_PATH: &str = "/api/benchmark/tune";

/// Agentic evaluation run (SSE).
pub const BENCHMARK_AGENTIC_PATH: &str = "/api/benchmark/agentic";

/// Setup status, used for the hardware snapshot on benchmark reports.
pub const SETUP_STATUS_PATH: &str = "/api/config/system/setup-status";

/// Apply a gated tune run, interpolating `run_id` into [`BENCHMARK_TUNE_PATH`].
#[must_use]
pub fn benchmark_tune_apply_path(run_id: i64) -> String {
    format!("{BENCHMARK_TUNE_PATH}/{run_id}/apply")
}

/// Every fixed path above, paired with the verbs the CLI sends to it.
///
/// The verb is half the contract: a deleted route often still *matches* some
/// parameterized sibling, and only the method it allows gives that away.
pub const CLI_ROUTE_CONTRACT: &[(&[&str], &str)] = &[
    (&["GET"], HEALTH_PATH),
    (&["POST"], PROXY_START_PATH),
    (&["POST"], PROXY_STOP_PATH),
    (&["GET"], PROXY_STATUS_PATH),
    (&["POST"], SERVERS_START_PATH),
    (&["POST"], DAEMON_SHUTDOWN_PATH),
    (&["GET", "POST"], DOWNLOADS_QUEUE_PATH),
    (&["GET"], MODELS_LIST_PATH),
    (&["POST"], BENCHMARK_COMPARE_PATH),
    (&["POST"], BENCHMARK_PERF_PATH),
    (&["POST"], BENCHMARK_TUNE_PATH),
    (&["POST"], BENCHMARK_AGENTIC_PATH),
    (&["GET"], SETUP_STATUS_PATH),
];

/// The verbs [`benchmark_tune_apply_path`] is called with.
pub const BENCHMARK_TUNE_APPLY_METHODS: &[&str] = &["POST"];

/// Every key the CLI puts in a `POST /api/proxy/start` body.
///
/// The two ends of that body cannot meet in one test. `StartProxyBody` is
/// `pub(crate)` inside `gglib-cli`'s `pub(crate) mod daemon_client`, and
/// `StartProxyConfig` is `pub(crate)` inside `gglib-axum`'s `pub(crate) mod
/// handlers`; both crates deny `unreachable_pub`, and gglib-axum may not depend
/// on gglib-cli. So each side pins itself against this list instead — the same
/// trick [`CLI_ROUTE_CONTRACT`] uses for paths.
pub const PROXY_START_CLI_FIELDS: &[&str] = &[
    "host",
    "port",
    "default_context",
    "cache",
    "slot_dir",
    "pinned",
    "cache_disk_gb",
    "inference_override",
    "default_profile",
    "api_key",
    "allowed_hosts",
];

/// Keys the daemon accepts on that body which the CLI never sends.
///
/// `llama_base_port` is read only by `POST /api/proxy/start-pinned`, which
/// routes it through the launch cascade. `/api/proxy/start` deserializes it and
/// never looks at it, so it is daemon-only by function rather than by omission.
pub const PROXY_START_DAEMON_ONLY_FIELDS: &[&str] = &["llama_base_port"];
