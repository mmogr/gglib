//! Tests for the companions an image model's download brings: linked by the
//! check a hand-made link passes, recorded as the model's files under their
//! absolute paths, and a refused or held one named.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::*;
use crate::domain::image_family_goldens::Golden;
use crate::domain::{ComponentRole, ImageFamily, Model, ModelComponent, ModelFile, NewModel};
use crate::download::Quantization;
use crate::paths::canonical_model_path;
use crate::ports::{GgufCapabilities, GgufMetadata, GgufParseError, ResolvedFile, TensorTable};
use crate::services::model_components::tests::{GoldenParser, write_golden};

/// Reads the weights' header as a Flux.1 file's, which holds no metadata
/// but its family, and every file's tensor table from the golden text it
/// holds.
struct FluxParser;

impl GgufParserPort for FluxParser {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        Ok(GgufMetadata {
            image_family: Some(ImageFamily::Flux1),
            ..GgufMetadata::default()
        })
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        GoldenParser.tensor_table(path)
    }
    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        unimplemented!("a registration reads files")
    }
}

/// Stores what is inserted, under id 1, and finds it by the path it holds.
/// A link the stored model has is kept when the inserted model carries none
/// in that role, as the `SQLite` repository's `INSERT OR IGNORE` keeps it.
#[derive(Default)]
struct OneSlotRepo(Mutex<Option<Model>>);

#[async_trait]
impl ModelRepository for OneSlotRepo {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(self.0.lock().unwrap().iter().cloned().collect())
    }
    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("id={id}")))
    }
    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("name={name}")))
    }
    async fn find_by_path(&self, path: &Path) -> Result<Option<Model>, RepositoryError> {
        let held = self.0.lock().unwrap().clone();
        Ok(held.filter(|model| model.file_path == path))
    }
    async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
        let mut stored = Model::stored(1, model);
        let held = self.0.lock().unwrap().take();
        if let Some(held) = held {
            for link in held.components {
                if !stored.components.iter().any(|kept| kept.role == link.role) {
                    stored.components.push(link);
                }
            }
        }
        stored.components.sort_by_key(|link| link.role);
        *self.0.lock().unwrap() = Some(stored.clone());
        Ok(stored)
    }
    async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Records each row inserted, as `(file_path, file_index, hf_oid)`.
#[derive(Default)]
struct RecordedRows(Mutex<Vec<(String, i32, Option<String>)>>);

#[async_trait]
impl ModelFilesRepositoryPort for RecordedRows {
    async fn insert(&self, file: &NewModelFile) -> Result<(), RepositoryError> {
        self.0
            .lock()
            .unwrap()
            .push((file.file_path.clone(), file.file_index, file.hf_oid.clone()));
        Ok(())
    }
    async fn get_by_model_id(&self, _model_id: i64) -> Result<Vec<ModelFile>, RepositoryError> {
        unimplemented!("a registration only stores rows")
    }
    async fn update_verification_time(
        &self,
        _id: i64,
        _at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        unimplemented!("a registration only stores rows")
    }
}

/// The weights and the three Flux.1 companions on disk under `models`, the
/// companions each in its repository's folder, and the download that
/// brought them. `vae` is the golden written where the VAE goes.
fn flux_download(models: &Path, vae: Golden) -> CompletedDownload {
    let model_dir = models.join("leejet_FLUX.1-schnell-gguf");
    std::fs::create_dir_all(&model_dir).unwrap();
    let weights = write_golden(&model_dir, Golden::FluxSchnellQ8);
    let place = |folder: &str, golden: Golden, name: &str| -> PathBuf {
        let dir = models.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let written = write_golden(&dir, golden);
        let path = dir.join(name);
        std::fs::rename(written, &path).unwrap();
        path
    };
    let components = vec![
        (
            ComponentRole::Vae,
            place("unsloth_FLUX.1-schnell", vae, "ae.safetensors"),
        ),
        (
            ComponentRole::ClipL,
            place(
                "comfyanonymous_flux_text_encoders",
                Golden::ClipL,
                "clip_l.safetensors",
            ),
        ),
        (
            ComponentRole::T5xxl,
            place(
                "comfyanonymous_flux_text_encoders",
                Golden::T5xxl,
                "t5xxl_fp16.safetensors",
            ),
        ),
    ];
    let entry = |role, repo: &str, path: &str| {
        ResolvedFile::companion(role, repo, path, 10, Some(format!("oid-{path}")))
    };
    CompletedDownload {
        all_paths: std::iter::once(weights.clone())
            .chain(components.iter().map(|(_, path)| path.clone()))
            .collect(),
        primary_path: weights,
        projector_path: None,
        components,
        quantization: Quantization::Q8_0,
        repo_id: "leejet/FLUX.1-schnell-gguf".to_owned(),
        commit_sha: "abc123".to_owned(),
        is_sharded: false,
        file_paths: None,
        hf_tags: vec![],
        hf_file_entries: vec![
            ResolvedFile::with_size_and_oid("flux1-schnell-q8_0.gguf", 7, Some("w".to_owned())),
            entry(
                ComponentRole::Vae,
                "unsloth/FLUX.1-schnell",
                "ae.safetensors",
            ),
            entry(
                ComponentRole::ClipL,
                "comfyanonymous/flux_text_encoders",
                "clip_l.safetensors",
            ),
            entry(
                ComponentRole::T5xxl,
                "comfyanonymous/flux_text_encoders",
                "t5xxl_fp16.safetensors",
            ),
        ],
    }
}

struct Registered {
    answer: RegisteredDownload,
    stored: Model,
    rows: Vec<(String, i32, Option<String>)>,
}

async fn register_into(repo: Arc<OneSlotRepo>, download: &CompletedDownload) -> Registered {
    let rows = Arc::new(RecordedRows::default());
    let registrar = ModelRegistrar::new(repo.clone(), Arc::new(FluxParser), Some(rows.clone()));
    let answer = registrar.register_model(download).await.unwrap();
    let stored = repo.0.lock().unwrap().clone().expect("a model is stored");
    let rows = rows.0.lock().unwrap().clone();
    Registered {
        answer,
        stored,
        rows,
    }
}

fn canonical(path: &Path) -> PathBuf {
    canonical_model_path(path).unwrap()
}

/// The family is read from the weights' header, each companion is linked
/// under its canonical path, and each gets a row under that absolute path
/// with its OID, after the weights' own row.
#[tokio::test]
async fn a_downloads_companions_are_linked_and_recorded_by_their_absolute_paths() {
    let models = tempfile::tempdir().unwrap();
    let download = flux_download(models.path(), Golden::FluxVae);

    let registered = register_into(Arc::default(), &download).await;

    assert_eq!(registered.stored.image_family, Some(ImageFamily::Flux1));
    assert!(registered.answer.component_refusals.is_empty());
    let expected: Vec<ModelComponent> = download
        .components
        .iter()
        .map(|(role, path)| ModelComponent {
            role: *role,
            path: canonical(path),
        })
        .collect();
    assert_eq!(registered.stored.components, expected);
    assert!(registered.stored.missing_components().is_empty());
    let mut rows = vec![(
        "flux1-schnell-q8_0.gguf".to_owned(),
        0,
        Some("w".to_owned()),
    )];
    for (index, (link, name)) in expected
        .iter()
        .zip([
            "ae.safetensors",
            "clip_l.safetensors",
            "t5xxl_fp16.safetensors",
        ])
        .enumerate()
    {
        let path = link.path.to_string_lossy().into_owned();
        assert!(Path::new(&path).is_absolute(), "{path}");
        rows.push((
            path,
            i32::try_from(index + 1).unwrap(),
            Some(format!("oid-{name}")),
        ));
    }
    assert_eq!(registered.rows, rows);
}

/// A file in the VAE's place that is not a Flux.1 VAE is refused for that
/// role: the model is registered all the same, with the other two linked,
/// the reason named, and no row for the file it does not draw with.
#[tokio::test]
async fn a_refused_companion_leaves_the_model_registered_with_the_reason() {
    let models = tempfile::tempdir().unwrap();
    let download = flux_download(models.path(), Golden::QwenImage21Vae);

    let registered = register_into(Arc::default(), &download).await;

    let roles: Vec<_> = registered
        .stored
        .components
        .iter()
        .map(|link| link.role)
        .collect();
    assert_eq!(roles, [ComponentRole::ClipL, ComponentRole::T5xxl]);
    assert_eq!(registered.stored.missing_components(), [ComponentRole::Vae]);
    let [refusal] = registered.answer.component_refusals.as_slice() else {
        panic!("one refusal: {:?}", registered.answer.component_refusals);
    };
    assert!(refusal.contains("ae.safetensors"), "{refusal}");
    assert!(refusal.contains("is not a vae for this model"), "{refusal}");
    assert!(
        registered
            .rows
            .iter()
            .all(|(path, ..)| !path.ends_with("ae.safetensors")),
        "{:?}",
        registered.rows
    );
    assert_eq!(registered.rows.len(), 3);
}

/// A model the library already holds, linked by hand to another VAE, keeps
/// that link when downloaded again, and the answer says so.
#[tokio::test]
async fn a_held_link_is_kept_when_the_model_is_downloaded_again() {
    let models = tempfile::tempdir().unwrap();
    let download = flux_download(models.path(), Golden::FluxVae);
    let mine = write_golden(models.path(), Golden::FluxVae);
    // The library holds a model under its resolved path.
    let mut held = NewModel::new(
        "flux".to_owned(),
        canonical(&download.primary_path),
        12.0,
        Utc::now(),
    );
    held.image_family = Some(ImageFamily::Flux1);
    held.components = vec![ModelComponent {
        role: ComponentRole::Vae,
        path: canonical(&mine),
    }];
    let repo = Arc::new(OneSlotRepo(Mutex::new(Some(Model::stored(1, &held)))));

    let registered = register_into(repo, &download).await;

    let vae = registered
        .stored
        .components
        .iter()
        .find(|link| link.role == ComponentRole::Vae)
        .expect("a VAE link");
    assert_eq!(vae.path, canonical(&mine));
    assert_eq!(
        registered.answer.component_refusals,
        [format!(
            "the model keeps its vae link to {}",
            canonical(&mine).display()
        )]
    );
    // Neither the VAE kept nor the one fetched is recorded as a file of the
    // model: the one it links was not this download's, and the one fetched
    // is not linked.
    let (_, fetched) = download
        .components
        .iter()
        .find(|(role, _)| *role == ComponentRole::Vae)
        .unwrap();
    for vae in [canonical(fetched), canonical(&mine)] {
        let vae = vae.to_string_lossy().into_owned();
        assert!(
            !registered.rows.iter().any(|(path, _, _)| *path == vae),
            "{vae} recorded: {:?}",
            registered.rows
        );
    }
}
