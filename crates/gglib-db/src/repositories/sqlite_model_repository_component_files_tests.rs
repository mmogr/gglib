//! A companion's `model_files` row, which the registrar records by the
//! absolute path the model links, goes with the link: an update that unlinks
//! the role or links another file drops it, so a repair never takes the
//! shared file for one of the model's own.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use gglib_core::domain::{ComponentRole, ImageFamily, ModelComponent, NewModelFile};
use gglib_core::ports::huggingface::fake_hub::{FakeHub, hub_file};
use gglib_core::ports::{AskedDownloads, ModelFilesRepositoryPort, NoopGgufParser};
use gglib_core::services::{ModelService, ModelVerificationService};
use gglib_core::{NewModel, paths::canonical_model_path};

use super::*;
use crate::repositories::ModelFilesRepository;
use crate::setup::setup_test_database;

const REPO: &str = "owner/zeta-GGUF";
const WEIGHTS: &str = "zeta.Q8_0.gguf";

/// The SHA-256 of `content`, for the two contents these rows record.
fn sha256(content: &str) -> String {
    match content {
        "weights" => "9a129038d9a00aed0cf6a7ea059ca50a813449061ab87848cf1a13eafdf33b2c",
        "vae" => "e089e84942af6414b066654f297dfbb11bf84b3e8b46d7252710076aff68d122",
        other => panic!("no digest for {other}"),
    }
    .to_owned()
}

/// A Flux.1 model downloaded from [`REPO`] into `dir`, linked to `vae`, with
/// a row for its weights and one for the VAE by its absolute path, as the
/// registrar records a companion.
async fn library(
    models: &Path,
    vae: &Path,
) -> (Arc<SqliteModelRepository>, Arc<ModelFilesRepository>, i64) {
    let pool = setup_test_database().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool.clone()));
    let files = Arc::new(ModelFilesRepository::new(pool));
    let dir = models.join("owner_zeta-GGUF");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(WEIGHTS), "weights").unwrap();
    let mut new = NewModel::new("flux".to_owned(), dir.join(WEIGHTS), 12.0, Utc::now());
    new.hf_repo_id = Some(REPO.to_owned());
    new.quantization = Some("Q8_0".to_owned());
    new.image_family = Some(ImageFamily::Flux1);
    let linked = canonical_model_path(vae).unwrap();
    new.components = vec![ModelComponent {
        role: ComponentRole::Vae,
        path: linked.clone(),
    }];
    let id = repo.insert(&new).await.unwrap().id;
    for (index, path, content) in [(0, PathBuf::from(WEIGHTS), "weights"), (1, linked, "vae")] {
        files
            .insert(&NewModelFile::new(
                id,
                path.to_string_lossy().into_owned(),
                index,
                1,
                Some(sha256(content)),
            ))
            .await
            .unwrap();
    }
    (repo, files, id)
}

/// A VAE in its own repository's folder, its bytes not the ones recorded.
fn damaged_vae(models: &Path) -> PathBuf {
    let folder = models.join("unsloth_FLUX.1-schnell");
    std::fs::create_dir_all(&folder).unwrap();
    let vae = folder.join("ae.safetensors");
    std::fs::write(&vae, "damaged").unwrap();
    vae
}

async fn rows(files: &ModelFilesRepository, id: i64) -> Vec<String> {
    files
        .get_by_model_id(id)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.file_path)
        .collect()
}

#[tokio::test]
async fn unlinking_or_relinking_a_component_drops_its_row_and_keeps_the_rest() {
    let models = tempfile::tempdir().unwrap();
    let vae = damaged_vae(models.path());
    let other = models.path().join("other-ae.safetensors");
    std::fs::write(&other, "vae").unwrap();

    // Relinked to another file.
    let (repo, files, id) = library(models.path(), &vae).await;
    let mut model = repo.get_by_id(id).await.unwrap();
    model.components[0].path = canonical_model_path(&other).unwrap();
    repo.update(&model).await.unwrap();
    assert_eq!(rows(&files, id).await, [WEIGHTS]);

    // Unlinked.
    let (repo, files, id) = library(models.path(), &vae).await;
    let mut model = repo.get_by_id(id).await.unwrap();
    model.components.clear();
    repo.update(&model).await.unwrap();
    assert_eq!(rows(&files, id).await, [WEIGHTS]);

    // An update that keeps the link keeps the row.
    let (repo, files, id) = library(models.path(), &vae).await;
    let mut model = repo.get_by_id(id).await.unwrap();
    model.name = "renamed".to_owned();
    repo.update(&model).await.unwrap();
    assert_eq!(rows(&files, id).await.len(), 2);
}

/// `gglib model update <id> --no-component vae`, then a repair: the VAE the
/// model no longer links is not its file, and nothing in the VAE's folder is
/// touched.
#[tokio::test]
async fn a_repair_after_an_unlink_deletes_nothing_in_the_companions_folder() {
    let models = tempfile::tempdir().unwrap();
    let vae = damaged_vae(models.path());
    let (repo, files, id) = library(models.path(), &vae).await;
    ModelService::new(repo.clone())
        .set_component(id, ComponentRole::Vae, None, &NoopGgufParser)
        .await
        .unwrap();
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        ..FakeHub::default()
    };
    let queued = Arc::new(AskedDownloads::default());
    let service = ModelVerificationService::new(repo, files, Arc::new(hub), queued.clone());

    let _ = service.repair_model(id, None).await;

    assert_eq!(std::fs::read_to_string(&vae).unwrap(), "damaged");
    assert!(queued.asked().is_empty());
}
