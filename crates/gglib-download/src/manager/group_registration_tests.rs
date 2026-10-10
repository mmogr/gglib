//! Tests for what the manager does as the files of a group that carries a
//! projector finish: when it registers the model, what it hands the
//! registrar, and what it announces.

use std::path::Path;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use gglib_core::RepositoryError;
use gglib_core::domain::{Model, ModelComponent, NewModel};
use gglib_core::ports::{CompletedDownload, ModelRegistrarPort, RegisteredDownload};

use super::test_support::{End, run_next};
use super::*;
use crate::test_hub::RepoHub;

const REPO: &str = "owner/zeta-GGUF";

/// Keeps each download it is asked to register, and answers
/// `metadata_refusal` as the reader's reason for refusing the weights,
/// `refusal` as the reason the projector was not linked, and
/// `component_refusals` as the companions not linked. Every companion the
/// download brought is linked to the model it answers, under its canonical
/// path as the library stores a link.
#[derive(Default)]
pub(super) struct RecordingRegistrar {
    pub(super) registered: StdMutex<Vec<CompletedDownload>>,
    pub(super) metadata_refusal: Option<String>,
    pub(super) refusal: Option<String>,
    pub(super) component_refusals: Vec<String>,
}

#[async_trait]
impl ModelRegistrarPort for RecordingRegistrar {
    async fn register_model(
        &self,
        download: &CompletedDownload,
    ) -> Result<RegisteredDownload, RepositoryError> {
        self.registered.lock().unwrap().push(download.clone());
        let mut new = NewModel::new(
            "zeta".to_string(),
            download.primary_path.clone(),
            7.0,
            chrono::Utc::now(),
        );
        new.components = download
            .components
            .iter()
            .map(|(role, path)| ModelComponent {
                role: *role,
                path: gglib_core::paths::canonical_model_path(path)
                    .unwrap_or_else(|_| path.clone()),
            })
            .collect();
        Ok(RegisteredDownload {
            model: Model::stored(1, &new),
            metadata_refusal: self.metadata_refusal.clone(),
            projector_refusal: self.refusal.clone(),
            component_refusals: self.component_refusals.clone(),
        })
    }
}

/// Keeps the text of every completed download announced.
#[derive(Default)]
struct Announced(StdMutex<Vec<String>>);

impl AppEventEmitter for Announced {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::Download {
            event: DownloadEvent::DownloadCompleted { text, .. },
        } = event
        {
            self.0.lock().unwrap().push(text);
        }
    }
}

struct Fixture {
    manager: DownloadManagerImpl,
    registrar: Arc<RecordingRegistrar>,
    announced: Arc<Announced>,
}

/// A manager with a `Q8_0` download of `files` queued.
async fn queued(files: &[(&str, u64)], refusal: Option<&str>) -> Fixture {
    let registrar = Arc::new(RecordingRegistrar {
        refusal: refusal.map(str::to_string),
        ..Default::default()
    });
    let announced = Arc::new(Announced::default());
    let manager = DownloadManagerImpl::new(
        registrar.clone(),
        Arc::new(RepoHub::new(files)),
        announced.clone(),
        DownloadManagerConfig::default(),
        Arc::new(gglib_core::ports::NoopGgufParser),
    );
    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();
    Fixture {
        manager,
        registrar,
        announced,
    }
}

impl Fixture {
    /// Runs the next queued file to the end, on disk, and answers its name.
    async fn finish_next(&self) -> String {
        run_next(&self.manager, End::OnDisk).await
    }

    fn registered(&self) -> Vec<CompletedDownload> {
        self.registrar.registered.lock().unwrap().clone()
    }
}

/// One weights file and a projector: the model is not registered at the
/// weights, and once the projector is in it is registered once, the weights
/// as its file and the projector apart.
#[tokio::test]
async fn the_model_is_registered_once_its_projector_is_in_and_handed_it_apart() {
    let f = queued(&[("mmproj-F16.gguf", 300), ("zeta.Q8_0.gguf", 1_000)], None).await;

    assert_eq!(f.finish_next().await, "zeta.Q8_0.gguf");
    assert!(f.registered().is_empty(), "the projector is still to come");
    assert_eq!(f.finish_next().await, "mmproj-F16.gguf");

    let registered = f.registered();
    assert_eq!(registered.len(), 1);
    let download = &registered[0];
    let models = Path::new("models");
    assert_eq!(download.primary_path, models.join("zeta.Q8_0.gguf"));
    assert_eq!(
        download.projector_path,
        Some(models.join("mmproj-F16.gguf"))
    );
    assert!(!download.is_sharded);
    assert_eq!(download.file_paths, None);
    assert_eq!(download.hf_file_entries.len(), 2, "a row for each file");
    let announced = f.announced.0.lock().unwrap().clone();
    assert_eq!(announced.len(), 1);
    assert!(
        announced[0].ends_with("with its projector mmproj-F16.gguf"),
        "{announced:?}"
    );
}

/// What the registrar answers about the projector reaches the announcement.
#[tokio::test]
async fn a_projector_the_registrar_did_not_link_is_announced_with_the_reason() {
    let files = [("mmproj-F16.gguf", 300), ("zeta.Q8_0.gguf", 1_000)];
    let f = queued(&files, Some("it holds a model's weights")).await;

    f.finish_next().await;
    f.finish_next().await;

    let announced = f.announced.0.lock().unwrap().clone();
    assert_eq!(announced.len(), 1);
    assert!(announced[0].contains("was not linked"), "{announced:?}");
    assert!(
        announced[0].contains("it holds a model's weights"),
        "{announced:?}"
    );
}

/// Three shards and a projector: the shards are the model's files, in
/// order, and the projector is none of them.
#[tokio::test]
async fn a_sharded_models_projector_is_not_one_of_its_shards() {
    let files = [
        ("mmproj-F16.gguf", 300),
        ("zeta.Q8_0-00001-of-00003.gguf", 1_000),
        ("zeta.Q8_0-00002-of-00003.gguf", 1_000),
        ("zeta.Q8_0-00003-of-00003.gguf", 500),
    ];
    let f = queued(&files, None).await;

    for _ in 0..3 {
        f.finish_next().await;
    }
    assert!(f.registered().is_empty(), "the projector is still to come");
    f.finish_next().await;

    let registered = f.registered();
    assert_eq!(registered.len(), 1);
    let download = &registered[0];
    let models = Path::new("models");
    let shards: Vec<_> = (1..=3)
        .map(|n| models.join(format!("zeta.Q8_0-0000{n}-of-00003.gguf")))
        .collect();
    assert_eq!(download.primary_path, shards[0]);
    assert!(download.is_sharded);
    assert_eq!(download.file_paths, Some(shards));
    assert_eq!(
        download.projector_path,
        Some(models.join("mmproj-F16.gguf"))
    );
    let announced = f.announced.0.lock().unwrap().clone();
    assert!(announced[0].contains("3 shards"), "{announced:?}");
}
