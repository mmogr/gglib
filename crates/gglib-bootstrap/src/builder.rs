//! [`CoreBootstrap`] — the shared composition root for all gglib adapters.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use gglib_core::ModelRegistrar;
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerConfig, DownloadManagerPort, GgufParserPort, HfClientPort,
    ModelRegistrarPort,
};
use gglib_core::services::AppCore;
use gglib_db::{CoreFactory, setup_database};
use gglib_download::{DownloadManagerDeps, build_download_manager};
// GGUF_BOOTSTRAP_EXCEPTION: Parser injected at composition root only
use gglib_gguf::GgufParser;
use gglib_hf::{DefaultHfClient, HfClientConfig};

use crate::built::BuiltCore;
use crate::config::BootstrapConfig;
use crate::download_trigger::DownloadTriggerAdapter;

/// What `build` makes from the Hub token, one for each thing that holds it.
struct TokenHolders {
    /// The Hub client's config. Search, browse and the registrar's recipe
    /// lookup ask through that client.
    client_config: HfClientConfig,
    /// The download manager's config, for its transfers.
    download_config: DownloadManagerConfig,
    /// `AppCore`'s token, for an upgrade's check and its download.
    core_token: Option<String>,
}

/// Each holder, handed `hf_token`: all three hold it, or none does.
fn token_holders(hf_token: Option<String>, models_dir: PathBuf) -> TokenHolders {
    TokenHolders {
        client_config: HfClientConfig::default().with_optional_token(hf_token.clone()),
        download_config: DownloadManagerConfig::new(models_dir).with_hf_token(hf_token.clone()),
        core_token: hf_token,
    }
}

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
    ///   Axum, `TauriEventEmitter` for Tauri, or `NoopEmitter` for the CLI,
    ///   tests and early init). Download events flow
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
        // 1. Database pool + repositories. The model-files repository among
        //    them is the one the registrar and the verification service share.
        let pool = setup_database(&config.db_path).await?;
        let repos = CoreFactory::build_repos(pool.clone());

        // 2. GGUF parser (shared: model registrar + capability detection)
        let gguf_parser: Arc<dyn GgufParserPort> = Arc::new(GgufParser::new());

        // 5. HuggingFace client. Built before the registrar because the
        //    registrar uses it to look up a model author's published sampling
        //    recipe at import time.
        //
        //    It carries the token, so a gated base repo — Llama and Gemma,
        //    routinely — can answer that lookup for a user who has
        //    configured one. Without it the lookup 401s and the import falls
        //    back to the tag guess, which is the designed degradation.
        //
        //    The token is read here and nowhere else, and no adapter is
        //    asked for it. `token_holders` hands it to this client, to the
        //    download manager's transfers (9) and to `AppCore` (11).
        let TokenHolders {
            client_config,
            download_config,
            core_token,
        } = token_holders(gglib_core::hf_token::from_env(), config.models_dir);
        let hf_client: Arc<dyn HfClientPort> = Arc::new(DefaultHfClient::new(&client_config));

        // 6. Model registrar — composes model repository + GGUF parser so
        //    that both GUI and CLI download paths use the identical
        //    registration logic.
        let model_registrar: Arc<dyn ModelRegistrarPort> = Arc::new(
            ModelRegistrar::new(
                repos.models.clone(),
                gguf_parser.clone(),
                Some(Arc::clone(&repos.model_files)),
            )
            .with_hf_client(hf_client.clone()),
        );

        // 9. Download manager, with the configuration made in (5)
        let downloads: Arc<dyn DownloadManagerPort> =
            Arc::new(build_download_manager(DownloadManagerDeps {
                model_registrar,
                hf_client: Arc::clone(&hf_client),
                event_emitter: emitter,
                config: download_config,
            }));

        // 10. Download trigger adapter (bridges DownloadManagerPort →
        //     DownloadTriggerPort for the verification service)
        let download_trigger = Arc::new(DownloadTriggerAdapter {
            download_manager: Arc::clone(&downloads),
        });

        // 11. AppCore, whose verification service checks for updates against
        //     the HF client and queues a repair through the trigger
        let app = Arc::new(
            AppCore::new(repos.clone(), hf_client.clone(), download_trigger)
                .with_hf_token(core_token),
        );

        tracing::debug!(
            db_path = %config.db_path.display(),
            "CoreBootstrap: infrastructure wired successfully"
        );

        Ok(BuiltCore {
            app,
            downloads,
            hf_client,
            gguf_parser,
            repos,
            pool,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a token of any account.
    const FAKE_TOKEN: &str = "hf_fake_token_for_a_test";

    #[test]
    fn a_token_is_handed_to_the_hub_client_the_download_manager_and_the_core() {
        let holders = token_holders(Some(FAKE_TOKEN.to_owned()), PathBuf::from("models"));

        assert!(holders.client_config.has_token());
        assert_eq!(
            holders.download_config.hf_token.as_deref(),
            Some(FAKE_TOKEN)
        );
        assert_eq!(holders.core_token.as_deref(), Some(FAKE_TOKEN));
    }

    #[test]
    fn with_no_token_none_of_the_three_is_handed_one() {
        let holders = token_holders(None, PathBuf::from("models"));

        assert!(!holders.client_config.has_token());
        assert_eq!(holders.download_config.hf_token, None);
        assert_eq!(holders.core_token, None);
    }

    #[test]
    fn the_download_manager_keeps_the_models_directory_it_is_given() {
        let holders = token_holders(None, PathBuf::from("somewhere/models"));

        assert_eq!(
            holders.download_config.models_directory,
            PathBuf::from("somewhere/models")
        );
    }
}
