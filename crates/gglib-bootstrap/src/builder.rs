//! [`CoreBootstrap`] — the shared composition root for all gglib adapters.

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
use sqlx::SqlitePool;

use crate::built::BuiltCore;
use crate::config::BootstrapConfig;

/// What `build` makes from the Hub token, one for each thing that holds it.
struct TokenHolders {
    /// The Hub client's config. Search, browse and the registrar's recipe
    /// lookup ask through that client.
    client_config: HfClientConfig,
    /// The download manager's config, for its transfers. It names no models
    /// directory: the manager asks for the current one as a download starts.
    download_config: DownloadManagerConfig,
    /// `AppCore`'s token, for an upgrade's check and its download.
    core_token: Option<String>,
}

/// Each holder, handed `hf_token`: all three hold it, or none does.
fn token_holders(hf_token: Option<String>) -> TokenHolders {
    TokenHolders {
        client_config: HfClientConfig::default().with_optional_token(hf_token.clone()),
        download_config: DownloadManagerConfig::default().with_hf_token(hf_token.clone()),
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
    /// * `config` — The database to open.
    /// * `emitter` — Adapter-specific event emitter: the daemon passes its
    ///   SSE broadcaster, and the CLI and tests a `NoopEmitter`, having no
    ///   listener. Download events flow through this emitter to the
    ///   adapter's transport.
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
        // 1. Database pool
        let pool = setup_database(&config.db_path).await?;

        // 2. HuggingFace client.
        //
        //    It carries the token, so a gated base repo — Llama and Gemma,
        //    routinely — can answer the registrar's recipe lookup for a user
        //    who has configured one. Without it the lookup 401s and the
        //    import falls back to the tag guess, which is the designed
        //    degradation.
        //
        //    The token is read here and nowhere else, and no adapter is
        //    asked for it. `token_holders` hands it to this client, to the
        //    download manager's transfers and to `AppCore`.
        let TokenHolders {
            client_config,
            download_config,
            core_token,
        } = token_holders(gglib_core::hf_token::from_env());
        let hf_client: Arc<dyn HfClientPort> = Arc::new(DefaultHfClient::new(&client_config));

        let built = wire(pool, hf_client, download_config, core_token, emitter);

        tracing::debug!(
            db_path = %config.db_path.display(),
            "CoreBootstrap: infrastructure wired successfully"
        );

        Ok(built)
    }
}

/// Everything [`CoreBootstrap::build`] wires once it has the database and the
/// Hub client.
fn wire(
    pool: SqlitePool,
    hf_client: Arc<dyn HfClientPort>,
    download_config: DownloadManagerConfig,
    core_token: Option<String>,
    emitter: Arc<dyn AppEventEmitter>,
) -> BuiltCore {
    // 3. Repositories. The model-files repository among them is the one the
    //    registrar and the verification service share.
    let repos = CoreFactory::build_repos(pool.clone());

    // 4. GGUF parser (shared: model registrar + capability detection)
    let gguf_parser: Arc<dyn GgufParserPort> = Arc::new(GgufParser::new());

    // 5. Model registrar — composes model repository + GGUF parser so that
    //    both GUI and CLI download paths use the identical registration
    //    logic. It holds the Hub client to look up a model author's
    //    published sampling recipe at import time.
    let model_registrar: Arc<dyn ModelRegistrarPort> = Arc::new(
        ModelRegistrar::new(
            repos.models.clone(),
            gguf_parser.clone(),
            Some(Arc::clone(&repos.model_files)),
        )
        .with_hf_client(hf_client.clone()),
    );

    // 6. Download manager
    let downloads: Arc<dyn DownloadManagerPort> =
        Arc::new(build_download_manager(DownloadManagerDeps {
            model_registrar,
            hf_client: Arc::clone(&hf_client),
            event_emitter: emitter,
            config: download_config,
        }));

    // 7. AppCore, whose verification service checks for updates against the
    //    HF client and queues a repair's download on that same manager
    let app = Arc::new(
        AppCore::new(repos.clone(), hf_client.clone(), Arc::clone(&downloads))
            .with_hf_token(core_token),
    );

    BuiltCore {
        app,
        downloads,
        hf_client,
        gguf_parser,
        repos,
        pool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a token of any account.
    const FAKE_TOKEN: &str = "hf_fake_token_for_a_test";

    #[test]
    fn a_token_is_handed_to_the_hub_client_the_download_manager_and_the_core() {
        let holders = token_holders(Some(FAKE_TOKEN.to_owned()));

        assert!(holders.client_config.has_token());
        assert_eq!(
            holders.download_config.hf_token.as_deref(),
            Some(FAKE_TOKEN)
        );
        assert_eq!(holders.core_token.as_deref(), Some(FAKE_TOKEN));
    }

    #[test]
    fn with_no_token_none_of_the_three_is_handed_one() {
        let holders = token_holders(None);

        assert!(!holders.client_config.has_token());
        assert_eq!(holders.download_config.hf_token, None);
        assert_eq!(holders.core_token, None);
    }

    /// A directory named here would be the one current as the adapter
    /// started, and every download of a daemon's life would go under it.
    #[test]
    fn the_download_manager_is_handed_no_models_directory_of_its_own() {
        let holders = token_holders(Some(FAKE_TOKEN.to_owned()));

        assert_eq!(holders.download_config.models_directory, None);
    }
}

#[cfg(test)]
#[path = "builder_repair_tests.rs"]
mod repair_tests;
