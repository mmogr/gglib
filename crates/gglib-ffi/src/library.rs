//! The model library an app opens: what it holds, and every call on it.
//!
//! A library is gglib's own core, built by gglib-bootstrap's composition
//! root over the folders the app names: the database in its data folder,
//! every download in its models folder, and the Hub asked with the token the
//! app hands over, or else with the `HF_TOKEN` its process started with,
//! which an app on a phone has none of. The engine it serves is composed
//! over its models, settings and MCP servers (`crate::serve`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{
    DownloadManagerPort, GgufParserPort, HfClientPort, ModelFilesRepositoryPort, ModelRepository,
};
use gglib_engine_llama::{Decoder, linked_llama_decoder};

use crate::browse::{self, QuantOffer, SearchPage, Suggestion};
use crate::download_feed::DownloadFeed;
use crate::download_watch::DownloadWatch;
use crate::downloads;
use crate::errors::GglibError;
use crate::installed::{self, InstalledModel};
use crate::reading::DeviceReading;
use crate::runtime::in_runtime;
use crate::serve::{self, Composition, LastServed, Server};

/// The name of the library's database in its data folder.
pub const DATABASE_FILE: &str = "gglib.db";

/// The folders a library keeps its files in, which the app makes and names.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LibraryDirs {
    /// Where the library's database lives.
    pub data_dir: String,
    /// Where every model is downloaded to, a folder per repository.
    pub models_dir: String,
}

/// The models on the device, the ways to get more, and the engine that
/// runs them.
///
/// The verdicts it gives are made against the [`DeviceReading`] it was last
/// handed by [`Self::suggested`], [`Self::quantizations`] or
/// [`Self::serve`]: one reading per browse, held for as long as its
/// verdicts are on screen, and one per serve, which the engine admits every
/// model against. [`Self::installed`] judges against the same one.
#[derive(uniffi::Object)]
pub struct Library {
    hub: Arc<dyn HfClientPort>,
    parser: Arc<dyn GgufParserPort>,
    downloads: Arc<dyn DownloadManagerPort>,
    /// The library's rows, which a test stores a model in as a download
    /// would, and the settings and MCP servers a served engine reads.
    pub(crate) composition: Composition,
    /// The rows of each model's files.
    pub(crate) model_files: Arc<dyn ModelFilesRepositoryPort>,
    /// What the download manager says, which a test reads to know a
    /// download has ended.
    pub(crate) feed: Arc<DownloadFeed>,
    models_dir: PathBuf,
    reading: Mutex<Option<DeviceReading>>,
    /// The engine served last: serving again stops it, or waits for it to
    /// stop when its server was let go of, and its model to be freed, so the
    /// device never holds two models.
    last_served: tokio::sync::Mutex<LastServed>,
}

/// Open the library in `dirs`, asking Hugging Face with `hf_token`.
///
/// With `None` it asks with the `HF_TOKEN` the process started with, which
/// an app on a phone has none of, so there it asks as nobody. Each folder is
/// made if it is missing.
///
/// # Errors
///
/// [`GglibError::NoSpace`] or [`GglibError::Internal`] when a folder cannot
/// be made or the database cannot be opened, as when a newer gglib wrote it.
#[uniffi::export]
pub async fn open_library(
    dirs: LibraryDirs,
    hf_token: Option<String>,
) -> Result<Arc<Library>, GglibError> {
    in_runtime(async move {
        let (config, models_dir) = config_for(&dirs, hf_token)?;
        let feed = Arc::new(DownloadFeed::new());
        let core = CoreBootstrap::build(config, Arc::clone(&feed) as _)
            .await
            .map_err(GglibError::internal)?;
        Ok(Arc::new(Library::over(core, feed, models_dir)))
    })
    .await
}

/// The bootstrap's config for `dirs`, once both folders exist, and the
/// models folder.
pub(crate) fn config_for(
    dirs: &LibraryDirs,
    hf_token: Option<String>,
) -> Result<(BootstrapConfig, PathBuf), GglibError> {
    let data_dir = PathBuf::from(&dirs.data_dir);
    let models_dir = PathBuf::from(&dirs.models_dir);
    for dir in [&data_dir, &models_dir] {
        std::fs::create_dir_all(dir).map_err(|e| GglibError::from_io(&e))?;
    }
    let config = BootstrapConfig {
        models_dir: Some(models_dir.clone()),
        hf_token,
        ..BootstrapConfig::for_db(data_dir.join(DATABASE_FILE))
    };
    Ok((config, models_dir))
}

impl Library {
    /// The library over `core`, whose download manager reports to `feed`.
    pub(crate) fn over(core: BuiltCore, feed: Arc<DownloadFeed>, models_dir: PathBuf) -> Self {
        Self {
            hub: core.hf_client,
            parser: core.gguf_parser,
            downloads: core.downloads,
            composition: Composition {
                models: core.repos.models,
                mcp_servers: core.repos.mcp_servers,
                settings: core.repos.settings,
            },
            model_files: core.repos.model_files,
            feed,
            models_dir,
            reading: Mutex::new(None),
            last_served: tokio::sync::Mutex::default(),
        }
    }

    /// The library's rows.
    pub(crate) fn models(&self) -> &dyn ModelRepository {
        self.composition.models.as_ref()
    }

    /// Serve an engine over `decoder` with its budget made from `reading`,
    /// once the engine served before has stopped and freed its model; hold
    /// `reading` for the verdicts that follow. A reading no budget can be
    /// made from is refused first, leaving the engine served before as it
    /// was.
    pub(crate) async fn serve_over(
        &self,
        decoder: impl Decoder + 'static,
        reading: DeviceReading,
    ) -> Result<Arc<Server>, GglibError> {
        serve::check_reading(reading)?;
        let mut last = self.last_served.lock().await;
        last.stop().await;
        let server = Arc::new(serve::start(decoder, &self.composition, reading).await?);
        self.hold(reading);
        *last = LastServed::of(&server);
        // Held until here, so two serves at once take turns.
        drop(last);
        Ok(server)
    }

    /// Hold `reading` for the verdicts that follow; answer its budget.
    fn hold(&self, reading: DeviceReading) -> Option<u64> {
        *self.reading.lock().unwrap_or_else(PoisonError::into_inner) = Some(reading);
        reading.budget()
    }

    /// The budget of the reading held, if there is one.
    fn held_budget(&self) -> Option<u64> {
        self.reading
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .and_then(|reading| reading.budget())
    }
}

#[uniffi::export]
impl Library {
    /// The models gglib suggests for a phone, each judged at `reading`,
    /// largest first, with why each. Asks nothing of the network, so it is
    /// the one call that answers at once.
    pub fn suggested(&self, reading: DeviceReading) -> Vec<Suggestion> {
        let budget = self.hold(reading);
        browse::suggestions(budget)
    }

    /// A page of Hugging Face's chat models matching `query`, from `cursor`
    /// (a page's `next_cursor`) or the first page.
    ///
    /// # Errors
    ///
    /// The Hub's: [`GglibError::AuthRequired`] when it refuses the token,
    /// [`GglibError::RateLimited`], [`GglibError::Network`].
    pub async fn search(
        self: Arc<Self>,
        query: String,
        cursor: Option<String>,
    ) -> Result<SearchPage, GglibError> {
        in_runtime(async move { browse::search(self.hub.as_ref(), &query, cursor).await }).await
    }

    /// Each quantization `repo` publishes, judged at `reading` from the
    /// first bytes of its file.
    ///
    /// # Errors
    ///
    /// The Hub's: [`GglibError::NotFound`], [`GglibError::AuthRequired`] for
    /// a repository that wants a token, [`GglibError::RateLimited`],
    /// [`GglibError::Network`].
    pub async fn quantizations(
        self: Arc<Self>,
        repo: String,
        reading: DeviceReading,
    ) -> Result<Vec<QuantOffer>, GglibError> {
        let budget = self.hold(reading);
        in_runtime(async move {
            let (hub, parser) = (Arc::clone(&self.hub), Arc::clone(&self.parser));
            browse::quantizations(hub, parser, &repo, budget).await
        })
        .await
    }

    /// The models on the device, judged at the reading the library holds:
    /// each [`crate::FitVerdict::Unknown`] for want of a budget until one is
    /// handed over.
    ///
    /// # Errors
    ///
    /// [`GglibError::Internal`] when the library cannot be read.
    pub async fn installed(self: Arc<Self>) -> Result<Vec<InstalledModel>, GglibError> {
        let budget = self.held_budget();
        in_runtime(async move { installed::installed(self.models(), budget).await }).await
    }

    /// Delete installed model `id`: its files, then its row.
    ///
    /// # Errors
    ///
    /// [`GglibError::NotFound`] for an id the library does not hold;
    /// [`GglibError::Internal`] when a file cannot be deleted, which leaves
    /// the model installed.
    pub async fn remove(self: Arc<Self>, id: i64) -> Result<(), GglibError> {
        in_runtime(async move {
            installed::remove(
                self.models(),
                self.model_files.as_ref(),
                &self.models_dir,
                id,
            )
            .await
        })
        .await
    }

    /// Download quantization `quantization` of `repo` into the models folder,
    /// and follow it.
    ///
    /// # Errors
    ///
    /// The Hub's, when it refuses the repository or quantization, and the
    /// download manager's, when it cannot queue it.
    pub async fn download(
        self: Arc<Self>,
        repo: String,
        quantization: String,
    ) -> Result<Arc<DownloadWatch>, GglibError> {
        in_runtime(async move {
            let (downloads, feed) = (Arc::clone(&self.downloads), Arc::clone(&self.feed));
            let watch =
                downloads::start(self.hub.as_ref(), downloads, feed, repo, quantization).await?;
            Ok(Arc::new(watch))
        })
        .await
    }

    /// Stop every download, keeping what each fetched, as the app leaves the
    /// foreground. Each one's watch answers [`crate::DownloadState::Paused`].
    ///
    /// Returns once each has stopped, so a resume straight after carries
    /// each on, or after five seconds of one still winding down. A download
    /// already putting its last file away is not paused: it ends done or
    /// failed, as it was going to.
    pub async fn pause_all(self: Arc<Self>) {
        in_runtime(async move { downloads::pause_all(self.downloads.as_ref(), &self.feed).await })
            .await;
    }

    /// Queue again what [`Self::pause_all`] stopped, in the order it stood;
    /// each carries on from what it fetched. One that cannot be queued
    /// again, with no network for one, answers
    /// [`crate::DownloadState::Failed`] and its watch ends. One still
    /// winding down from the pause stays paused, for the next resume.
    pub async fn resume_all(self: Arc<Self>) {
        in_runtime(async move { downloads::resume_all(&self.downloads, &self.feed).await }).await;
    }

    /// Serve the engine: gglib's proxy on a port of 127.0.0.1, in front of
    /// llama.cpp in this process, which loads the model a request names,
    /// fitted to the budget gglib makes from `reading`. The reading is held
    /// for the whole serve; an engine served before is stopped first, and
    /// its model freed.
    ///
    /// # Errors
    ///
    /// [`GglibError::EngineUnavailable`] when this build has no llama.cpp to
    /// run a model with: anything but a build of the framework for an Apple
    /// platform. [`GglibError::Internal`] for a reading with a zero in it,
    /// which no model can be fitted to and which leaves an engine served
    /// before serving, and when a port cannot be had.
    pub async fn serve(self: Arc<Self>, reading: DeviceReading) -> Result<Arc<Server>, GglibError> {
        in_runtime(async move {
            let decoder = linked_llama_decoder(None).ok_or(GglibError::EngineUnavailable)?;
            self.serve_over(decoder, reading).await
        })
        .await
    }
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
