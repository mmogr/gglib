//! Model registrar service implementation.
//!
//! This service implements `ModelRegistrarPort` using the `ModelRepository`
//! and `GgufParserPort` dependencies. It's used by the download manager
//! to register completed downloads.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use super::model_components::link_downloaded_components;
use super::model_import::fetch_published_sampling;
use super::model_links::resolved_or_literal;
use super::model_projector::link_downloaded_projector;
use super::{HfOrigin, ModelOrigin, build_new_model};
use crate::domain::{Model, NewModelFile};
use crate::ports::huggingface::HfClientPort;
use crate::ports::{
    CompletedDownload, GgufParserPort, ModelFilesRepositoryPort, ModelRegistrarPort,
    ModelRepository, RegisteredDownload, RepositoryError,
};

/// Implementation of the model registrar port.
///
/// This service composes over `ModelRepository` for persistence and
/// `GgufParserPort` for metadata extraction.
pub struct ModelRegistrar {
    /// Repository for persisting models.
    model_repo: Arc<dyn ModelRepository>,
    /// Parser for extracting GGUF metadata.
    gguf_parser: Arc<dyn GgufParserPort>,
    /// Repository for persisting model file metadata.
    model_files_repo: Option<Arc<dyn ModelFilesRepositoryPort>>,
    /// Looks up the model author's published sampling recipe.
    ///
    /// Optional, and absent means "do not look" rather than "cannot register".
    /// A registrar without one applies the `reasoning` tag guess. That keeps
    /// the feature off in tests and in any embedding that has no HF client,
    /// without either having to know it exists.
    hf_client: Option<Arc<dyn HfClientPort>>,
}

impl ModelRegistrar {
    /// Create a new model registrar.
    ///
    /// # Arguments
    ///
    /// * `model_repo` - Repository for persisting models
    /// * `gguf_parser` - Parser for extracting GGUF metadata
    /// * `model_files_repo` - Optional repository for persisting model file metadata
    pub fn new(
        model_repo: Arc<dyn ModelRepository>,
        gguf_parser: Arc<dyn GgufParserPort>,
        model_files_repo: Option<Arc<dyn ModelFilesRepositoryPort>>,
    ) -> Self {
        Self {
            model_repo,
            gguf_parser,
            model_files_repo,
            hf_client: None,
        }
    }

    /// Look up published sampling recipes at import, using `client`.
    ///
    /// A builder method rather than a fourth constructor parameter: only the
    /// application wiring has an HF client to give, and every other call site
    /// wants no lookup.
    #[must_use]
    pub fn with_hf_client(mut self, client: Arc<dyn HfClientPort>) -> Self {
        self.hf_client = Some(client);
        self
    }
}

#[async_trait]
impl ModelRegistrarPort for ModelRegistrar {
    async fn register_model(
        &self,
        download: &CompletedDownload,
    ) -> Result<RegisteredDownload, RepositoryError> {
        let file_path = download.db_path();

        // Parse GGUF metadata from the downloaded file. A file the reader
        // refuses is registered all the same, without the details its header
        // would have given: the reader may be stricter than llama.cpp. Its
        // words go back to the caller.
        let (gguf_metadata, metadata_refusal) = match self.gguf_parser.parse(file_path) {
            Ok(metadata) => (Some(metadata), None),
            Err(refused) => {
                tracing::warn!(
                    path = %file_path.display(),
                    reason = %refused,
                    "GGUF metadata not read - registering the model without it"
                );
                (None, Some(refused.to_string()))
            }
        };

        // Best-effort, and deliberately before the row is built: a recipe the
        // author published is better evidence than the tag guess
        // `build_new_model` would otherwise write. Returns `None` for every
        // failure — gated repo, offline, nothing published — and the import
        // proceeds unchanged. See `fetch_published_sampling`.
        let published = match &self.hf_client {
            Some(client) => {
                fetch_published_sampling(client.as_ref(), &download.repo_id, &download.hf_tags)
                    .await
            }
            None => None,
        };

        let origin = ModelOrigin::HuggingFace(HfOrigin {
            repo_id: &download.repo_id,
            commit_sha: &download.commit_sha,
            hf_tags: &download.hf_tags,
            quantization_fallback: download.quantization,
            file_paths: download.file_paths.as_deref(),
            published_sampling: published.as_ref(),
        });
        let mut model = build_new_model(
            file_path,
            gguf_metadata.as_ref(),
            self.gguf_parser.as_ref(),
            &origin,
            Utc::now(),
        );

        // A projector that fails the link's check leaves the model registered
        // without a link, and the reason goes back to the caller.
        let projector_refusal = link_downloaded_projector(
            self.model_repo.as_ref(),
            &mut model,
            download.projector_path.as_deref(),
            self.gguf_parser.as_ref(),
        )
        .await;

        // The companions an image model's download brought pass the check a
        // hand-made link passes; one refused, or one the model already holds
        // another file for, leaves the model registered without it.
        let component_refusals = link_downloaded_components(
            self.model_repo.as_ref(),
            &mut model,
            &download.components,
            self.gguf_parser.as_ref(),
        )
        .await;

        let registered = self.model_repo.insert(&model).await?;

        // One model_files record, with its OID, per file of the group (if repo is available)
        if let Some(ref repo) = self.model_files_repo {
            for (file_index, file_entry) in download.hf_file_entries.iter().enumerate() {
                let Some(file_path) = model_file_path(download, &registered, file_entry) else {
                    continue;
                };
                if let Some(size) = file_entry.size {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    let model_file = NewModelFile::new(
                        registered.id,
                        file_path,
                        file_index as i32,
                        size as i64,
                        file_entry.oid.clone(),
                    );

                    if let Err(e) = repo.insert(&model_file).await {
                        // Soft fail - log but don't propagate error
                        tracing::warn!(
                            model_id = registered.id,
                            file_path = %file_entry.path,
                            error = %e,
                            "Failed to insert model_files record - verification features may be unavailable"
                        );
                    }
                }
            }
        }

        Ok(RegisteredDownload {
            model: registered,
            metadata_refusal,
            projector_refusal,
            component_refusals,
        })
    }
}

/// The path `entry`'s `model_files` record holds, or `None` when it gets
/// none.
///
/// The model's own files are recorded by their path in the repository, as
/// verification joins it to the model's folder. A companion lives in its own
/// repository's folder, so it is recorded by its absolute canonical path,
/// which that join keeps as it is, and only when the model is linked to the
/// very file the download brought: a companion the model was not linked to
/// is not one of its files.
fn model_file_path(
    download: &CompletedDownload,
    registered: &Model,
    entry: &crate::ports::ResolvedFile,
) -> Option<String> {
    let Some(role) = entry.component else {
        return Some(entry.path.clone());
    };
    let (_, downloaded) = download
        .components
        .iter()
        .find(|(downloaded_role, _)| *downloaded_role == role)?;
    let linked = registered
        .components
        .iter()
        .find(|link| link.role == role)?;
    (resolved_or_literal(downloaded) == linked.path)
        .then(|| linked.path.to_string_lossy().into_owned())
}

#[cfg(test)]
#[path = "model_registrar_metadata_tests.rs"]
mod metadata_tests;

#[cfg(test)]
#[path = "model_registrar_projector_tests.rs"]
mod projector_tests;

#[cfg(test)]
#[path = "model_registrar_components_tests.rs"]
mod components_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Model, NewModel};
    use crate::download::Quantization;
    use crate::ports::NoopGgufParser;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Mock model repository for testing.
    struct MockModelRepo {
        models: Mutex<Vec<Model>>,
        next_id: Mutex<i64>,
    }

    impl MockModelRepo {
        fn new() -> Self {
            Self {
                models: Mutex::new(Vec::new()),
                next_id: Mutex::new(1),
            }
        }
    }

    #[async_trait]
    impl ModelRepository for MockModelRepo {
        async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
            Ok(self.models.lock().unwrap().clone())
        }

        async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
            self.models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.id == id)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(format!("id={id}")))
        }

        async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
            self.models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.name == name)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(format!("name={name}")))
        }

        async fn find_by_path(
            &self,
            path: &std::path::Path,
        ) -> Result<Option<Model>, RepositoryError> {
            Ok(self
                .models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.file_path.as_path() == path)
                .cloned())
        }

        async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
            let mut id = self.next_id.lock().unwrap();
            let persisted = Model::stored(*id, model);
            // Mirror the `SQLite` repository: a repeat registration of the
            // same file updates that row and keeps its id. This double models
            // registration-after-download, which is the very path the trait
            // doc cites as the reason `insert` upserts — a double that appends
            // contradicts the contract it exists to stand in for.
            let mut models = self.models.lock().unwrap();
            if let Some(index) = models
                .iter()
                .position(|m| m.file_path == persisted.file_path)
            {
                let mut updated = persisted.clone();
                updated.id = models[index].id;
                models[index] = updated.clone();
                drop(models);
                return Ok(updated);
            }
            *id += 1;
            drop(id);
            models.push(persisted.clone());
            drop(models);
            Ok(persisted)
        }

        async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
            Ok(())
        }

        async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_register_model_basic() {
        let repo = Arc::new(MockModelRepo::new());
        let parser = Arc::new(NoopGgufParser);
        let registrar = ModelRegistrar::new(repo.clone(), parser, None);

        let download = CompletedDownload {
            primary_path: PathBuf::from("/models/test-model-q4_k_m.gguf"),
            all_paths: vec![PathBuf::from("/models/test-model-q4_k_m.gguf")],
            projector_path: None,
            components: vec![],
            quantization: Quantization::Q4KM,
            repo_id: "test/model".to_string(),
            commit_sha: "abc123".to_string(),
            is_sharded: false,
            file_paths: None,
            hf_tags: vec![],
            hf_file_entries: vec![],
        };

        let result = registrar.register_model(&download).await;
        assert!(result.is_ok());

        let model = result.unwrap().model;
        assert_eq!(model.name, "model");
        assert_eq!(model.hf_repo_id, Some("test/model".to_string()));
        assert_eq!(model.hf_commit_sha, Some("abc123".to_string()));
        assert_eq!(model.quantization, Some("Q4_K_M".to_string()));
    }

    #[tokio::test]
    async fn test_register_sharded_model() {
        let repo = Arc::new(MockModelRepo::new());
        let parser = Arc::new(NoopGgufParser);
        let registrar = ModelRegistrar::new(repo.clone(), parser, None);

        let download = CompletedDownload {
            primary_path: PathBuf::from("/models/llama-00001-of-00004.gguf"),
            all_paths: vec![
                PathBuf::from("/models/llama-00001-of-00004.gguf"),
                PathBuf::from("/models/llama-00002-of-00004.gguf"),
                PathBuf::from("/models/llama-00003-of-00004.gguf"),
                PathBuf::from("/models/llama-00004-of-00004.gguf"),
            ],
            projector_path: None,
            components: vec![],
            quantization: Quantization::Q8_0,
            repo_id: "test/llama".to_string(),
            commit_sha: "def456".to_string(),
            is_sharded: true,
            file_paths: None,
            hf_tags: vec![],
            hf_file_entries: vec![],
        };

        let result = registrar.register_model(&download).await;
        assert!(result.is_ok());

        let model = result.unwrap().model;
        assert_eq!(model.quantization, Some("Q8_0".to_string()));
        assert_eq!(model.name, "llama");
    }
}
