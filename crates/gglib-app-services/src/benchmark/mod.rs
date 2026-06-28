//! Benchmark service — shared between CLI and web adapters.
//!
//! # Module Layout
//!
//! ```text
//! benchmark/
//!   mod.rs     — BenchmarkOps, BenchmarkDeps (public API)
//!   compare.rs — SSE inference loop: ModelRuntimePort orchestration +
//!                defensive stream parsing
//!   perf.rs    — llama-bench process spawning + VRAM drain logic
//!   mapper.rs  — raw serde_json::Value → domain type transforms
//!   guard.rs   — BenchmarkTaskGuard (DropCancels pattern for HTTP layer)
//! ```
//!
//! # VRAM Contention Prevention
//!
//! `BenchmarkDeps::runtime` is the **same** [`ModelRuntimePort`] instance
//! shared with `ProxyOps` at the composition root (created once in
//! `bootstrap.rs`).  Both operations go through the same `SingleSwap`
//! [`ProcessManager`], which guarantees only one llama-server runs at a time
//! system-wide.  `run_perf()` additionally calls `stop_current()` before
//! spawning `llama-bench` so that the GPU is free when the binary loads the
//! model directly.
//!
//! # Defensive Parsing Contract
//!
//! All JSON-to-domain-type transforms are delegated to [`mapper`].  Timing
//! fields are `Option<f64>`; a missing or malformed `timings` object in
//! llama-server's SSE response produces `None` for every timing field — never
//! a panic or hard error.
//!
//! [`ModelRuntimePort`]: gglib_core::ports::ModelRuntimePort
//! [`ProcessManager`]: gglib_runtime::process::ProcessManager

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::benchmark::{BenchmarkEvent, CompareConfig, PerfConfig};
use gglib_core::ports::{
    BenchmarkRepositoryPort, ModelRepository, ModelRuntimePort, SettingsRepository,
};

mod compare;
pub mod guard;
pub mod mapper;
mod perf;

// ────────────────────────────────────────────────────────────────────────────
// Dependency bundle
// ────────────────────────────────────────────────────────────────────────────

/// All external dependencies needed by [`BenchmarkOps`].
///
/// # HTTP client timeout
///
/// The `http_client` **must** be built with an explicit long timeout (≥ 10
/// minutes) because large models can have a very long time-to-first-token
/// (TTFT).  Do **not** reuse the short-timeout client from `AxumContext` or
/// `TauriContext`.
///
/// ```rust,ignore
/// let http_client = reqwest::Client::builder()
///     .timeout(Duration::from_secs(600))
///     .build()?;
/// ```
pub struct BenchmarkDeps {
    /// Model catalog for name and file-path lookups.
    pub model_repo: Arc<dyn ModelRepository>,
    /// Shared [`ModelRuntimePort`] — same instance used by `ProxyOps`.
    ///
    /// Sharing this ensures SingleSwap semantics: only one llama-server can
    /// run at any time system-wide.
    pub runtime: Arc<dyn ModelRuntimePort>,
    /// Benchmark persistence (runs, results, summaries).
    pub bench_repo: Arc<dyn BenchmarkRepositoryPort>,
    /// HTTP client with a ≥ 10-minute timeout for compare-mode SSE streaming.
    pub http_client: reqwest::Client,
    /// Settings repository used to read `default_context_size` and global
    /// `inference_defaults` at the start of each compare run — mirrors the
    /// same per-request settings read the proxy performs.
    pub settings_repo: Arc<dyn SettingsRepository>,
}

impl BenchmarkDeps {
    /// Construct a dedicated `reqwest::Client` with the required 10-minute
    /// timeout for TTFT on large models.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest` cannot build the client (extremely rare —
    /// only fails on TLS initialisation errors).
    pub fn build_http_client() -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build benchmark HTTP client: {e}"))
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Service struct
// ────────────────────────────────────────────────────────────────────────────

/// Benchmark service shared by CLI and HTTP adapters.
///
/// Constructed once at bootstrap and injected into both the CLI handler and
/// the Axum HTTP handler.  All heavy lifting is delegated to [`compare`] and
/// [`perf`] submodules.
pub struct BenchmarkOps {
    deps: BenchmarkDeps,
}

impl BenchmarkOps {
    /// Create a new `BenchmarkOps` from its dependency bundle.
    pub fn new(deps: BenchmarkDeps) -> Self {
        Self { deps }
    }

    /// Run a compare benchmark: stream the same prompt through N models
    /// sequentially, emit [`BenchmarkEvent`]s on `tx`.
    ///
    /// The caller must pass a [`CancellationToken`] that fires when the client
    /// disconnects (HTTP) or receives `Ctrl+C` (CLI).  The loop checks the
    /// token cooperatively between models; on cancellation it calls
    /// `stop_current()` and marks the run as `Failed`.
    pub async fn run_compare(
        &self,
        config: CompareConfig,
        tx: Sender<BenchmarkEvent>,
        cancel: CancellationToken,
    ) -> Result<()> {
        compare::run_compare(&self.deps, config, tx, cancel).await
    }

    /// Run a perf benchmark: invoke `llama-bench` on each model sequentially,
    /// emit [`BenchmarkEvent`]s on `tx`.
    ///
    /// Before each model, `stop_current()` is called to drain VRAM so that
    /// `llama-bench` can load the model cleanly.
    pub async fn run_perf(
        &self,
        config: PerfConfig,
        tx: Sender<BenchmarkEvent>,
        cancel: CancellationToken,
    ) -> Result<()> {
        perf::run_perf(&self.deps, config, tx, cancel).await
    }
}
