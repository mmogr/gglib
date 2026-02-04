//! Model registrar service implementation.
//!
//! This service implements `ModelRegistrarPort` using the `ModelRepository`
//! and `GgufParserPort` dependencies. It's used by the download manager
//! to register completed downloads.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use crate::domain::{Model, NewModel};
use crate::download::Quantization;
use crate::ports::{
    CompletedDownload, GgufParserPort, ModelRegistrarPort, ModelRepository, RepositoryError,
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
}

impl ModelRegistrar {
    /// Create a new model registrar.
    ///
    /// # Arguments
    ///
    /// * `model_repo` - Repository for persisting models
    /// * `gguf_parser` - Parser for extracting GGUF metadata
    pub fn new(model_repo: Arc<dyn ModelRepository>, gguf_parser: Arc<dyn GgufParserPort>) -> Self {
        Self {
            model_repo,
            gguf_parser,
        }
    }
}

#[async_trait]
impl ModelRegistrarPort for ModelRegistrar {
    async fn register_model(&self, download: &CompletedDownload) -> Result<Model, RepositoryError> {
        let file_path = download.db_path();

        // Parse GGUF metadata from the downloaded file
        let gguf_metadata = self.gguf_parser.parse(file_path).ok();

        // Extract param_count_b from metadata, fall back to 0.0
        let param_count_b = gguf_metadata
            .as_ref()
            .and_then(|m| m.param_count_b)
            .unwrap_or(0.0);

        let mut model = NewModel::new(
            download.repo_id.clone(),
            file_path.to_path_buf(),
            param_count_b,
            Utc::now(),
        );

        // Use extracted metadata where available, with fallbacks
        model.quantization = gguf_metadata
            .as_ref()
            .and_then(|m| m.quantization.clone())
            .or_else(|| Some(download.quantization.to_string()));
        model.architecture = gguf_metadata.as_ref().and_then(|m| m.architecture.clone());
        model.context_length = gguf_metadata.as_ref().and_then(|m| m.context_length);
        if let Some(ref meta) = gguf_metadata {
            model.metadata.clone_from(&meta.metadata);
        }
        model.hf_repo_id = Some(download.repo_id.clone());
        model.hf_commit_sha = Some(download.commit_sha.clone());
        model.hf_filename = Some(file_path.file_name().unwrap().to_string_lossy().to_string());
        model.download_date = Some(Utc::now());

        // Pass through file_paths for sharded models
        model.file_paths.clone_from(&download.file_paths);

        // Auto-detect capabilities from metadata
        if let Some(ref meta) = gguf_metadata {
            let capabilities = self.gguf_parser.detect_capabilities(meta);
            model.tags = capabilities.to_tags();
        }

        // Infer model capabilities from chat template
        let template = model.metadata.get("tokenizer.chat_template");
        let name = model.metadata.get("general.name");
        model.capabilities = crate::domain::infer_from_chat_template(
            template.map(String::as_str),
            name.map(String::as_str),
        );

        let registered = self.model_repo.insert(&model).await?;

        Ok(registered)
    }

    async fn register_model_from_path(
        &self,
        repo_id: &str,
        commit_sha: &str,
        file_path: &Path,
        quantization: &str,
    ) -> Result<Model, RepositoryError> {
        let download = CompletedDownload {
            primary_path: file_path.to_path_buf(),
            all_paths: vec![file_path.to_path_buf()],
            quantization: Quantization::from_filename(quantization),
            repo_id: repo_id.to_string(),
            commit_sha: commit_sha.to_string(),
            is_sharded: false,
            total_bytes: 0,
            file_paths: None,
        };

        self.register_model(&download).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Model;
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

        async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
            let mut id = self.next_id.lock().unwrap();
            let persisted = Model {
                id: *id,
                name: model.name.clone(),
                file_path: model.file_path.clone(),
                param_count_b: model.param_count_b,
                architecture: model.architecture.clone(),
                quantization: model.quantization.clone(),
                context_length: model.context_length,
                metadata: model.metadata.clone(),
                added_at: model.added_at,
                hf_repo_id: model.hf_repo_id.clone(),
                hf_commit_sha: model.hf_commit_sha.clone(),
                hf_filename: model.hf_filename.clone(),
                capabilities: model.capabilities,
                download_date: model.download_date,
                last_update_check: model.last_update_check,
                tags: model.tags.clone(),
                inference_defaults: model.inference_defaults.clone(),
            };
            *id += 1;
            drop(id);
            self.models.lock().unwrap().push(persisted.clone());
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
        let registrar = ModelRegistrar::new(repo.clone(), parser);

        let download = CompletedDownload {
            primary_path: PathBuf::from("/models/test-model-q4_k_m.gguf"),
            all_paths: vec![PathBuf::from("/models/test-model-q4_k_m.gguf")],
            quantization: Quantization::Q4KM,
            repo_id: "test/model".to_string(),
            commit_sha: "abc123".to_string(),
            is_sharded: false,
            total_bytes: 1024,
            file_paths: None,
        };

        let result = registrar.register_model(&download).await;
        assert!(result.is_ok());

        let model = result.unwrap();
        assert_eq!(model.name, "test/model");
        assert_eq!(model.hf_repo_id, Some("test/model".to_string()));
        assert_eq!(model.hf_commit_sha, Some("abc123".to_string()));
        assert_eq!(model.quantization, Some("Q4_K_M".to_string()));
    }

    #[tokio::test]
    async fn test_register_sharded_model() {
        let repo = Arc::new(MockModelRepo::new());
        let parser = Arc::new(NoopGgufParser);
        let registrar = ModelRegistrar::new(repo.clone(), parser);

        let download = CompletedDownload {
            primary_path: PathBuf::from("/models/llama-00001-of-00004.gguf"),
            all_paths: vec![
                PathBuf::from("/models/llama-00001-of-00004.gguf"),
                PathBuf::from("/models/llama-00002-of-00004.gguf"),
                PathBuf::from("/models/llama-00003-of-00004.gguf"),
                PathBuf::from("/models/llama-00004-of-00004.gguf"),
            ],
            quantization: Quantization::Q8_0,
            repo_id: "test/llama".to_string(),
            commit_sha: "def456".to_string(),
            is_sharded: true,
            total_bytes: 4096,
            file_paths: None,
        };

        let result = registrar.register_model(&download).await;
        assert!(result.is_ok());

        let model = result.unwrap();
        assert_eq!(model.quantization, Some("Q8_0".to_string()));
    }

    #[tokio::test]
    async fn test_register_model_from_path() {
        let repo = Arc::new(MockModelRepo::new());
        let parser = Arc::new(NoopGgufParser);
        let registrar = ModelRegistrar::new(repo.clone(), parser);

        let result = registrar
            .register_model_from_path(
                "test/repo",
                "commit123",
                Path::new("/models/test-q4_0.gguf"),
                "Q4_0",
            )
            .await;

        assert!(result.is_ok());
        let model = result.unwrap();
        assert_eq!(model.name, "test/repo");
    }
}
