//! [`CoreBootstrap`] — the shared composition root for all gglib adapters.

use std::sync::Arc;

use anyhow::Result;

use gglib_core::ports::{
    AppEventEmitter, DownloadManagerConfig, DownloadManagerPort, GgufParserPort, HfClientPort,
    ModelRegistrarPort,
};
use gglib_core::services::AppCore;
use gglib_core::{ModelRegistrar, hf_token};
use gglib_db::{CoreFactory, setup_database};
use gglib_download::{DownloadManagerDeps, build_download_manager};
// GGUF_BOOTSTRAP_EXCEPTION: Parser injected at composition root only
use gglib_gguf::GgufParser;
use gglib_hf::{DefaultHfClient, HfClientConfig};
use sqlx::SqlitePool;

use crate::built::BuiltCore;
use crate::config::BootstrapConfig;

/// What `build` makes from the config's models directory and the Hub token,
/// one for each thing that holds the token.
struct Holders {
    /// The Hub client's config. Search, browse and the registrar's recipe
    /// lookup ask through that client.
    client_config: HfClientConfig,
    /// The download manager's config, for its transfers and the directory
    /// they go under. With no directory the manager asks for the current one
    /// as a download starts.
    download_config: DownloadManagerConfig,
    /// `AppCore`'s token, for an upgrade's check and its download.
    core_token: Option<String>,
}

/// Each holder, for `config`: the download manager's config names its models
/// directory, and all three hold one token, or none does.
///
/// The token is the config's, read by [`hf_token::normalised`]; when the
/// config holds none, or a blank one, it is what `env_token` reads, and
/// `env_token` is not called otherwise.
fn holders(config: &BootstrapConfig, env_token: impl FnOnce() -> Option<String>) -> Holders {
    let token = config
        .hf_token
        .as_deref()
        .and_then(hf_token::normalised)
        .or_else(env_token);
    Holders {
        client_config: HfClientConfig::default().with_optional_token(token.clone()),
        download_config: DownloadManagerConfig {
            models_directory: config.models_dir.clone(),
            ..DownloadManagerConfig::default().with_hf_token(token.clone())
        },
        core_token: token,
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
    /// * `config` — The database to open, and the models directory and Hub
    ///   token of a caller that has its own.
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
        // The HuggingFace client.
        //
        // It carries the token, so a gated base repo — Llama and Gemma,
        // routinely — can answer the registrar's recipe lookup for a user
        // who has configured one. Without it the lookup 401s and the import
        // falls back to the tag guess, which is the designed degradation.
        //
        // A token the caller hands over is the one wired, unless it is
        // blank, and the environment is not read. Otherwise it is the
        // environment's, read here and nowhere else. `holders` hands it to
        // this client, to the download manager's transfers and to `AppCore`.
        let holders = holders(&config, hf_token::from_env);
        let hf_client: Arc<dyn HfClientPort> =
            Arc::new(DefaultHfClient::new(&holders.client_config));
        Self::build_over(config, holders, hf_client, emitter).await
    }

    /// [`Self::build`], over a Hub client the caller made rather than the
    /// one the config gives: everything that asks the Hub asks `hf_client`,
    /// and a download fetches each file from the address it gives
    /// (`HfClientPort::file_url`). A test's stand-in Hub is one.
    ///
    /// The token is read as [`Self::build`] reads it and handed to the
    /// download manager's transfers and to `AppCore`; what `hf_client` sends
    /// is its own affair.
    ///
    /// # Errors
    ///
    /// As [`Self::build`].
    pub async fn build_with_hub(
        config: BootstrapConfig,
        emitter: Arc<dyn AppEventEmitter>,
        hf_client: Arc<dyn HfClientPort>,
    ) -> Result<BuiltCore> {
        let holders = holders(&config, hf_token::from_env);
        Self::build_over(config, holders, hf_client, emitter).await
    }

    /// Open the database and wire everything over `hf_client`.
    async fn build_over(
        config: BootstrapConfig,
        holders: Holders,
        hf_client: Arc<dyn HfClientPort>,
        emitter: Arc<dyn AppEventEmitter>,
    ) -> Result<BuiltCore> {
        let pool = setup_database(&config.db_path).await?;

        let Holders {
            client_config: _,
            download_config,
            core_token,
        } = holders;
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

    // 4. GGUF parser (shared: model registrar, the download manager's head
    //    read of an image model's weights, and capability detection)
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
            gguf_parser: gguf_parser.clone(),
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
    use std::path::PathBuf;

    use super::*;

    /// A token a caller hands the bootstrap. Not a token of any account.
    const HANDED_TOKEN: &str = "hf_fake_token_handed_to_the_bootstrap";

    /// The token in the environment. Not a token of any account.
    const ENV_TOKEN: &str = "hf_fake_token_in_the_environment";

    /// A config for a database, with `models_dir` and `hf_token`.
    fn config(models_dir: Option<PathBuf>, hf_token: Option<&str>) -> BootstrapConfig {
        BootstrapConfig {
            models_dir,
            hf_token: hf_token.map(str::to_owned),
            ..BootstrapConfig::for_db(PathBuf::from("/data/gglib.db"))
        }
    }

    /// An environment that must not be read.
    fn unread() -> Option<String> {
        unreachable!("the environment was read though the config holds a token")
    }

    /// The one token the Hub client, the download manager and the core all
    /// hold, or `None` when none of them holds one. The client's config
    /// shows only whether it has a token.
    fn the_token(holders: &Holders) -> Option<&str> {
        let token = holders.core_token.as_deref();
        assert_eq!(holders.download_config.hf_token.as_deref(), token);
        assert_eq!(holders.client_config.has_token(), token.is_some());
        token
    }

    #[test]
    fn a_token_in_the_config_is_wired_and_the_environment_is_not_read() {
        let holders = holders(&config(None, Some(HANDED_TOKEN)), unread);

        assert_eq!(the_token(&holders), Some(HANDED_TOKEN));
    }

    #[test]
    fn the_spaces_around_a_token_in_the_config_are_dropped() {
        let handed = format!("  {HANDED_TOKEN} \n");

        let holders = holders(&config(None, Some(&handed)), unread);

        assert_eq!(the_token(&holders), Some(HANDED_TOKEN));
    }

    /// A blank token is read as `HF_TOKEN`'s is: no token, so the config
    /// holds none of its own.
    #[test]
    fn with_a_blank_token_in_the_config_the_environments_is_wired() {
        for handed in ["", "  \t\n"] {
            let holders = holders(&config(None, Some(handed)), || Some(ENV_TOKEN.to_owned()));

            assert_eq!(the_token(&holders), Some(ENV_TOKEN), "{handed:?}");
        }
    }

    #[test]
    fn with_no_token_in_the_config_the_environments_is_wired() {
        let holders = holders(&config(None, None), || Some(ENV_TOKEN.to_owned()));

        assert_eq!(the_token(&holders), Some(ENV_TOKEN));
    }

    #[test]
    fn with_no_token_anywhere_none_of_the_three_is_handed_one() {
        let holders = holders(&config(None, None), || None);

        assert_eq!(the_token(&holders), None);
    }

    /// The daemon names none. A directory named for it would be the one
    /// current as it started, and every download of its life would go under
    /// it.
    #[test]
    fn the_download_manager_is_handed_no_models_directory_of_its_own() {
        let config = BootstrapConfig::for_db(PathBuf::from("/data/gglib.db"));

        let holders = holders(&config, || Some(ENV_TOKEN.to_owned()));

        assert_eq!(holders.download_config.models_directory, None);
    }

    #[test]
    fn a_models_directory_in_the_config_is_the_download_managers() {
        let models_dir = PathBuf::from("/container/models");

        let holders = holders(&config(Some(models_dir.clone()), None), || None);

        assert_eq!(holders.download_config.models_directory, Some(models_dir));
    }
}

#[cfg(test)]
#[path = "builder_repair_tests.rs"]
mod repair_tests;
