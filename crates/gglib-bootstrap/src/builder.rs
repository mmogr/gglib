//! [`CoreBootstrap`] — the shared composition root for all gglib adapters.

use std::sync::Arc;

use anyhow::Result;

use gglib_core::ModelRegistrar;
use gglib_core::ports::{
    AppEventBridge, AppEventEmitter, DownloadManagerConfig, DownloadManagerPort, GgufParserPort,
    HfClientPort, ModelRegistrarPort, ModelRepository, ProcessRunner,
};
use gglib_core::services::{AppCore, ModelVerificationService};
use gglib_db::{CoreFactory, ModelFilesRepository, setup_database};
use gglib_download::{DownloadManagerDeps, build_download_manager};
// GGUF_BOOTSTRAP_EXCEPTION: Parser injected at composition root only
use gglib_gguf::GgufParser;
use gglib_hf::{DefaultHfClient, HfClientConfig};
use gglib_runtime::LlamaServerRunner;

use crate::built::BuiltCore;
use crate::config::BootstrapConfig;
use crate::download_trigger::DownloadTriggerAdapter;

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
    ///   Axum, `TauriEventEmitter` for Tauri, `CliDownloadEventEmitter` for
    ///   CLI, or `NoopEmitter` for tests/early init). Download events flow
    ///   through this emitter to the adapter's transport.
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
