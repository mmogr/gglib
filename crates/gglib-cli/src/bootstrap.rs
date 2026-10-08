//! CLI bootstrap - the composition root for the CLI adapter.
//!
//! Shared infrastructure (DB, download manager, model registrar,
//! verification service, …) is wired by [`gglib_bootstrap::CoreBootstrap`].
//! This module is the only place where CLI-specific concerns are added on
//! top: the console the progress bars draw on, the MCP service, and the
//! shared HTTP client.

use std::sync::Arc;

use anyhow::Result;
use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerPort, GgufParserPort, HfClientPort, ModelCatalogPort,
    NoopEmitter, SettingsRepository,
};
use gglib_core::services::AppCore;
use gglib_db::{SqliteBenchmarkRepository, SqliteLoopGuardTripLog};
use gglib_mcp::McpService;
use gglib_runtime::CatalogPortImpl;

use crate::console::CliConsole;
use crate::daemon_client::LibraryChanges;

// Path utilities from core
use gglib_core::paths::{database_path, resolve_models_dir};

/// Fully composed application context for CLI commands.
///
/// This struct owns all the infrastructure and provides access to
/// the `AppCore` for command handlers.
pub struct CliContext {
    /// The core application facade.
    pub app: Arc<AppCore>,
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// Download manager for model downloads.
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// The one `HuggingFace` client, holding the Hub token: the client the
    /// registrar and the download manager ask through, and the one
    /// `model search` and `browse` search with.
    pub hf_client: Arc<dyn HfClientPort>,
    /// GGUF parser for file validation and metadata extraction.
    pub gguf_parser: Arc<dyn GgufParserPort>,
    /// Shared model catalog, for `gglib_core::request_pipeline::resolve`.
    ///
    /// Commands that compose an agent loop need the target model's
    /// capabilities, tags and inference defaults; resolving them through this
    /// port is what keeps the CLI in step with the proxy.
    pub catalog: Arc<dyn ModelCatalogPort>,
    /// Shared HTTP client for LLM adapter calls.
    ///
    /// Constructed once at bootstrap and cloned into each agent session so that
    /// TCP connections to llama-server are pooled across REPL turns, matching
    /// the connection-pooling behaviour of the Axum handler.
    pub http_client: reqwest::Client,
    /// Benchmark run repository for compare and perf results.
    pub bench_repo: Arc<SqliteBenchmarkRepository>,
    /// The loop guard's log, read straight from this machine's database, so
    /// `gglib proxy trips` answers whether or not a daemon is running.
    pub loop_guard_trips: Arc<SqliteLoopGuardTripLog>,
    /// Settings repository for user preferences and inference defaults.
    pub settings_repo: Arc<dyn SettingsRepository>,
    /// The console progress bars are drawn on.
    ///
    /// The download monitors draw the queue on it, and the interactive one
    /// suspends it while prompting for additional model IDs.
    pub console: Arc<CliConsole>,
    /// What this command's `ModelOps` change in the library, kept for the
    /// daemon that serves it: `handlers::model::one_shot_model_ops` emits
    /// into it, and `handlers::model::dispatch` has the daemon told.
    pub(crate) library_changes: Arc<LibraryChanges>,
}

/// Bootstrap the CLI application.
///
/// Delegates all shared wiring to [`CoreBootstrap::build`] and adds the
/// CLI-specific layer: the console, the MCP service, and the shared HTTP
/// client.
pub async fn bootstrap() -> Result<CliContext> {
    // Resolve paths/env up-front so BootstrapConfig holds only resolved data.
    let models_resolution = resolve_models_dir(None)?;
    let bootstrap_config = BootstrapConfig {
        db_path: database_path()?,
        models_dir: models_resolution.path,
    };
    bootstrap_with(bootstrap_config).await
}

/// [`bootstrap`] over the database and models directory `bootstrap_config`
/// names, so a test can point it at a temporary directory.
pub(crate) async fn bootstrap_with(bootstrap_config: BootstrapConfig) -> Result<CliContext> {
    // The console owns the progress bars and routes log lines around them.
    // The CLI subscribes to no application events: a download monitor reads
    // the queue snapshot it draws, from the daemon or from the download
    // manager, so the shared bootstrap is handed an emitter that drops them.
    let console = Arc::new(CliConsole::new());
    let emitter: Arc<dyn AppEventEmitter> = Arc::new(NoopEmitter::new());

    let BuiltCore {
        app,
        downloads,
        hf_client,
        gguf_parser,
        repos,
        pool,
    } = CoreBootstrap::build(bootstrap_config, emitter).await?;

    let bench_repo = Arc::new(SqliteBenchmarkRepository::new(pool.clone()));
    let loop_guard_trips = Arc::new(SqliteLoopGuardTripLog::new(pool));

    let mcp = Arc::new(McpService::new(repos.mcp_servers.clone()));

    Ok(CliContext {
        app,
        mcp,
        downloads,
        hf_client,
        gguf_parser,
        catalog: Arc::new(CatalogPortImpl::new(repos.models)),
        http_client: gglib_proxy::loopback::client(),
        bench_repo,
        loop_guard_trips,
        settings_repo: repos.settings,
        console,
        library_changes: Arc::default(),
    })
}

/// [`bootstrap_with`] over a fresh database and models directory in `dir`,
/// for a test.
#[cfg(test)]
pub(crate) async fn test_context(dir: &std::path::Path) -> CliContext {
    let models_dir = dir.join("models");
    std::fs::create_dir_all(&models_dir).expect("models dir");
    bootstrap_with(BootstrapConfig {
        db_path: dir.join("gglib.db"),
        models_dir,
    })
    .await
    .expect("the database opens")
}
