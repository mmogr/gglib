//! Shared composition root for gglib adapters.
//!
//! This crate consolidates the common infrastructure-wiring steps that were
//! previously duplicated across the CLI, Axum, and Tauri bootstrap modules:
//!
//! 1. Database pool + repository set
//! 2. Process runner (`LlamaServerRunner`)
//! 3. GGUF parser + model-files repository + model registrar
//! 4. Download manager (accepting an injected event emitter)
//! 5. `DownloadTriggerAdapter` (bridges `DownloadManagerPort` → `DownloadTriggerPort`)
//! 6. `ModelVerificationService` + fully wired `AppCore`
//!
//! Each adapter then adds its own concerns on top of the returned [`BuiltCore`]
//! (MCP service, proxy supervisor, SSE broadcaster, etc.).
//!
//! # Hexagonal boundary
//!
//! This crate depends **only** on pure infrastructure crates
//! (`gglib-core`, `gglib-db`, `gglib-download`, `gglib-gguf`, `gglib-hf`,
//! `gglib-runtime`). It does **not** depend on adapter crates (`gglib-mcp`,
//! `gglib-axum`, `gglib-tauri`, `gglib-cli`).
//!
//! # Example
//!
//! ```ignore
//! use std::sync::Arc;
//! use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
//! use gglib_core::ports::AppEventEmitter;
//!
//! let emitter: Arc<dyn AppEventEmitter> = Arc::new(MyEmitter::new());
//! let config = BootstrapConfig {
//!     db_path: database_path()?,
//!     llama_server_path: llama_server_path()?,
//!     max_concurrent: 4,
//!     models_dir: resolve_models_dir(None)?.path,
//!     hf_token: std::env::var("HF_TOKEN").ok(),
//! };
//! let core = CoreBootstrap::build(config, emitter).await?;
//! // core.app, core.runner, core.downloads, core.hf_client, … all ready
//! ```

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use gglib_core::ModelRegistrar;
// tokio is a required runtime dependency (async fn build uses it transitively)
use gglib_core::download::DownloadError;
use gglib_core::ports::{
    AppEventBridge, AppEventEmitter, DownloadManagerConfig, DownloadManagerPort, GgufParserPort,
    HfClientPort, ModelRegistrarPort, ModelRepository, ProcessRunner, Repos,
};
use gglib_core::services::{AppCore, ModelVerificationService};
use gglib_db::{CoreFactory, ModelFilesRepository, setup_database};
use gglib_download::{DownloadManagerDeps, build_download_manager};
use tokio as _;
// GGUF_BOOTSTRAP_EXCEPTION: Parser injected at composition root only
use gglib_gguf::GgufParser;
use gglib_hf::{DefaultHfClient, HfClientConfig};
use gglib_runtime::LlamaServerRunner;

// ---------------------------------------------------------------------------
// Public configuration types
// ---------------------------------------------------------------------------

/// Configuration required to run [`CoreBootstrap::build`].
///
/// All paths must be fully resolved by the caller before passing this struct.
/// The path-resolution helpers (`database_path`, `llama_server_path`,
/// `resolve_models_dir`) are deliberately kept in `gglib-core::paths` so
/// that adapters own their own path strategies.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Absolute path to the SQLite database file.
    pub db_path: PathBuf,
    /// Absolute path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum number of concurrently running llama-server processes.
    pub max_concurrent: usize,
    /// Absolute path to the directory where model files are stored.
    pub models_dir: PathBuf,
    /// Optional HuggingFace API token for authenticated downloads.
    pub hf_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Output type
// ---------------------------------------------------------------------------

/// Fully wired infrastructure produced by [`CoreBootstrap::build`].
///
/// All fields are `pub` so adapter bootstrap modules can access every
/// service they need to assemble their own context structs.
pub struct BuiltCore {
    /// Core application facade with verification service attached.
    pub app: Arc<AppCore>,
    /// Process runner for llama-server lifecycle management.
    pub runner: Arc<dyn ProcessRunner>,
    /// Download manager trait object.
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// HuggingFace HTTP client.
    pub hf_client: Arc<dyn HfClientPort>,
    /// GGUF file parser for metadata extraction and capability detection.
    pub gguf_parser: Arc<dyn GgufParserPort>,
    /// Repository set (models, settings, MCP servers, chat history).
    ///
    /// Adapters need this to construct the MCP service and other
    /// infrastructure that requires direct repository access.
    pub repos: Repos,
    /// Model registrar shared between the download manager and direct
    /// registration code paths (e.g., CLI `model add` command).
    pub model_registrar: Arc<dyn ModelRegistrarPort>,
}

// ---------------------------------------------------------------------------
// DownloadTriggerAdapter (consolidated from CLI + Axum bootstrap copies)
// ---------------------------------------------------------------------------

/// Bridges [`DownloadManagerPort`] → [`DownloadTriggerPort`].
///
/// `ModelVerificationService` needs a `DownloadTriggerPort` to queue
/// downloads, but the download manager implements the richer
/// `DownloadManagerPort`. This adapter performs the conversion.
struct DownloadTriggerAdapter {
    download_manager: Arc<dyn DownloadManagerPort>,
}

#[async_trait]
impl gglib_core::services::DownloadTriggerPort for DownloadTriggerAdapter {
    async fn queue_download(
        &self,
        repo_id: String,
        quantization: Option<String>,
    ) -> anyhow::Result<String> {
        use gglib_core::download::Quantization;
        use gglib_core::ports::DownloadRequest;
        use std::str::FromStr;

        // Convert quantization string to enum; default to Q4_K_M when absent.
        let quant = quantization
            .as_ref()
            .and_then(|q| Quantization::from_str(q).ok())
            .unwrap_or(Quantization::Q4KM);

        let request = DownloadRequest::new(repo_id, quant);
        let id = self
            .download_manager
            .queue_download(request)
            .await
            .map_err(|e: DownloadError| anyhow::anyhow!("Failed to queue download: {e}"))?;

        Ok(id.to_string())
    }
}

// ---------------------------------------------------------------------------
// CoreBootstrap
// ---------------------------------------------------------------------------

/// Shared composition root that wires common infrastructure for all adapters.
///
/// Call [`CoreBootstrap::build`] once at adapter startup. The returned
/// [`BuiltCore`] contains every shared service; adapter-specific concerns
/// (MCP service, SSE broadcaster, proxy supervisor, etc.) are added on top
/// by the individual adapter bootstrap modules.
pub struct CoreBootstrap;

impl CoreBootstrap {
    /// Build and wire all shared infrastructure.
    ///
    /// # Arguments
    ///
    /// * `config` — Resolved paths and runtime parameters.
    /// * `emitter` — Adapter-specific event emitter (SSE broadcaster for
    ///   Axum/Tauri, `AppEventBridge` wrapping a `TauriEventEmitter`, or
    ///   `NoopEmitter` for CLI). Download events flow through this emitter
    ///   to the frontend.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened, the schema
    /// migration fails, or any infrastructure component cannot be
    /// initialized.
    pub async fn build(
        config: BootstrapConfig,
        emitter: Arc<dyn AppEventEmitter>,
    ) -> Result<BuiltCore> {
        // 1. Database pool + repositories
        let pool = setup_database(&config.db_path).await?;
        let repos = CoreFactory::build_repos(pool.clone());

        // 2. Process runner
        let runner: Arc<dyn ProcessRunner> = Arc::new(LlamaServerRunner::new(
            config.llama_server_path,
            config.max_concurrent,
        ));

        // 3. GGUF parser (shared: model registrar + capability detection)
        let gguf_parser: Arc<dyn GgufParserPort> = Arc::new(GgufParser::new());

        // 4. Model-files repository (used by registrar + verification service)
        let model_files_repo = Arc::new(ModelFilesRepository::new(pool.clone()));

        // 5. Model registrar — composes model repository + GGUF parser so
        //    that both GUI and CLI download paths use the identical
        //    registration logic.
        // Keep the concrete type so it satisfies the Sized bound in
        // DownloadManagerDeps<R, ..>; erased to trait object only in BuiltCore.
        let model_registrar_concrete = Arc::new(ModelRegistrar::new(
            repos.models.clone(),
            gguf_parser.clone(),
            Some(Arc::clone(&model_files_repo)
                as Arc<dyn gglib_core::services::ModelFilesRepositoryPort>),
        ));
        let model_registrar: Arc<dyn ModelRegistrarPort> = model_registrar_concrete.clone();

        // 6. Download manager configuration
        let download_config = {
            let mut cfg = DownloadManagerConfig::new(config.models_dir);
            if let Some(token) = config.hf_token {
                cfg = cfg.with_hf_token(Some(token));
            }
            cfg
        };

        // 7. HuggingFace client
        let hf_client_concrete = Arc::new(DefaultHfClient::new(&HfClientConfig::default()));
        let hf_client: Arc<dyn HfClientPort> = hf_client_concrete.clone();

        // 8. Download state repository
        let download_repo = CoreFactory::download_state_repository(pool);

        // 9. Download manager — `DownloadManagerDeps<R,..>` requires R: Sized,
        //    so we pass the concrete registrar. The emitter is bridged from the
        //    adapter's AppEventEmitter to satisfy DownloadEventEmitterPort.
        let download_emitter = Arc::new(AppEventBridge::new(Arc::clone(&emitter)));
        let downloads: Arc<dyn DownloadManagerPort> =
            Arc::new(build_download_manager(DownloadManagerDeps {
                model_registrar: model_registrar_concrete,
                download_repo,
                hf_client: hf_client_concrete,
                event_emitter: download_emitter,
                config: download_config,
            }));

        // 10. Download trigger adapter (bridges DownloadManagerPort →
        //     DownloadTriggerPort for ModelVerificationService)
        let download_trigger = Arc::new(DownloadTriggerAdapter {
            download_manager: Arc::clone(&downloads),
        });

        // 11. Model verification service
        let model_repo: Arc<dyn ModelRepository> = repos.models.clone();
        let verification_service = Arc::new(ModelVerificationService::new(
            Arc::clone(&model_repo),
            Arc::clone(&model_files_repo) as Arc<dyn gglib_core::services::ModelFilesReaderPort>,
            hf_client.clone(),
            download_trigger,
        ));

        // 12. AppCore — fully wired with verification
        let app = Arc::new(
            AppCore::new(repos.clone(), Arc::clone(&runner))
                .with_verification(verification_service),
        );

        tracing::debug!(
            db_path = %config.db_path.display(),
            "CoreBootstrap: infrastructure wired successfully"
        );

        Ok(BuiltCore {
            app,
            runner,
            downloads,
            hf_client,
            gguf_parser,
            repos,
            model_registrar,
        })
    }
}
