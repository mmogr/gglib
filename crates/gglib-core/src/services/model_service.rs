//! Model service - orchestrates model CRUD operations.

use crate::domain::{Model, NewModel};
use crate::ports::{CoreError, GgufParserPort, ModelRepository, RepositoryError};
use std::path::Path;
use std::sync::Arc;

/// The diff produced by [`ModelService::retag_model`] when at least one tag
/// changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetagDiff {
    /// Tags that were newly added.
    pub added: Vec<String>,
    /// Tags that were removed (only non-empty on a `full = true` rebuild).
    pub removed: Vec<String>,
}

impl RetagDiff {
    /// Returns `true` if any tag was added or removed.
    pub const fn is_changed(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }
}

/// Service for model operations.
///
/// This service provides high-level model management by delegating
/// to the injected `ModelRepository`. It adds no business logic
/// beyond what the repository provides - it's a thin facade.
pub struct ModelService {
    repo: Arc<dyn ModelRepository>,
}

impl ModelService {
    /// Create a new model service with the given repository.
    pub fn new(repo: Arc<dyn ModelRepository>) -> Self {
        Self { repo }
    }

    /// List all models.
    pub async fn list(&self) -> Result<Vec<Model>, CoreError> {
        self.repo.list().await.map_err(CoreError::from)
    }

    /// Get a model by its identifier (id, name, or HF ID).
    pub async fn get(&self, identifier: &str) -> Result<Option<Model>, CoreError> {
        // Try by ID first
        if let Ok(id) = identifier.parse::<i64>() {
            match self.repo.get_by_id(id).await {
                Ok(model) => return Ok(Some(model)),
                Err(RepositoryError::NotFound(_)) => {}
                Err(e) => return Err(CoreError::from(e)),
            }
        }
        // Try by name
        match self.repo.get_by_name(identifier).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::from(e)),
        }
    }

    /// Get a model by its database ID.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<Model>, CoreError> {
        match self.repo.get_by_id(id).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::from(e)),
        }
    }

    /// Get a model by name.
    pub async fn get_by_name(&self, name: &str) -> Result<Option<Model>, CoreError> {
        match self.repo.get_by_name(name).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::from(e)),
        }
    }

    /// Find a model by identifier (id, name, or HF ID).
    /// Returns error if not found.
    pub async fn find_by_identifier(&self, identifier: &str) -> Result<Model, CoreError> {
        self.get(identifier)
            .await?
            .ok_or_else(|| CoreError::Validation(format!("Model not found: {identifier}")))
    }

    /// Resolve a model identifier to its tag list.
    ///
    /// Returns an empty `Vec` when the identifier is unknown or the lookup
    /// fails — callers use this for read-only side-channel inputs (e.g.
    /// dialect selection at compose time) where a missing model should
    /// fall back to default behaviour rather than abort the request.
    pub async fn tags_for(&self, identifier: &str) -> Vec<String> {
        match self.get(identifier).await {
            Ok(Some(m)) => m.tags,
            _ => Vec::new(),
        }
    }

    /// Find a model by name. Returns error if not found.
    pub async fn find_by_name(&self, name: &str) -> Result<Model, CoreError> {
        self.get_by_name(name)
            .await?
            .ok_or_else(|| CoreError::Validation(format!("Model not found: {name}")))
    }

    /// Add a new model.
    pub async fn add(&self, model: NewModel) -> Result<Model, CoreError> {
        self.repo.insert(&model).await.map_err(CoreError::from)
    }

    /// Import a model from a local GGUF file with full metadata extraction.
    ///
    /// Validates file, parses GGUF metadata, detects capabilities, and registers
    /// with rich metadata. This is the canonical way to add local models.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Absolute path to the GGUF file
    /// * `gguf_parser` - Parser implementation for metadata extraction
    /// * `param_count_override` - Optional user override for parameter count
    ///
    /// # Returns
    ///
    /// Returns the registered `Model` with full metadata, or validation error.
    ///
    /// # Design
    ///
    /// This method orchestrates:
    /// 1. File validation (existence, extension)
    /// 2. GGUF metadata parsing (architecture, quantization, context)
    /// 3. Capability detection (reasoning, tool-calling from metadata)
    /// 4. Chat template inference (additional capability signals)
    /// 5. Auto-tag generation from detected capabilities
    /// 6. Model persistence with complete `NewModel` struct
    pub async fn import_from_file(
        &self,
        file_path: &Path,
        gguf_parser: &dyn GgufParserPort,
        param_count_override: Option<f64>,
    ) -> Result<Model, CoreError> {
        // 1. Validate and parse GGUF file
        let gguf_metadata = crate::utils::validation::validate_and_parse_gguf(
            gguf_parser,
            file_path
                .to_str()
                .ok_or_else(|| CoreError::Validation("Invalid file path encoding".to_string()))?,
        )
        .map_err(|e| CoreError::Validation(format!("GGUF validation failed: {e}")))?;

        // 2. Resolve parameter count (override > metadata > 0.0 fallback)
        let param_count_b = param_count_override
            .or(gguf_metadata.param_count_b)
            .unwrap_or(0.0);

        // 3. Detect capabilities from GGUF metadata
        let gguf_capabilities = gguf_parser.detect_capabilities(&gguf_metadata);
        let auto_tags = gguf_capabilities.to_tags();

        // 4. Infer capabilities from chat template OR architecture, whichever
        //    provides signal.  Architecture-based inference is the backstop for
        //    models whose GGUF ships without a tokenizer section (common in
        //    stripped quantisation builds, e.g. many Mistral/Devstral releases).
        let template = gguf_metadata.metadata.get("tokenizer.chat_template");
        let name = gguf_metadata.metadata.get("general.name");
        let from_template = crate::domain::infer_from_chat_template(
            template.map(String::as_str),
            name.map(String::as_str),
        );
        let from_arch =
            crate::domain::capabilities_from_architecture(gguf_metadata.architecture.as_deref());
        let model_capabilities = from_template | from_arch;

        // 5. Construct fully-populated NewModel
        let new_model = NewModel {
            name: name.cloned().unwrap_or_else(|| {
                file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown Model")
                    .to_string()
            }),
            file_path: file_path.to_path_buf(),
            param_count_b,
            architecture: gguf_metadata.architecture,
            quantization: gguf_metadata.quantization,
            context_length: gguf_metadata.context_length,
            expert_count: gguf_metadata.expert_count,
            expert_used_count: gguf_metadata.expert_used_count,
            expert_shared_count: gguf_metadata.expert_shared_count,
            metadata: gguf_metadata.metadata,
            added_at: chrono::Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: auto_tags,
            file_paths: None,
            capabilities: model_capabilities,
            inference_defaults: None,
        };

        // 6. Persist to repository
        self.repo.insert(&new_model).await.map_err(CoreError::from)
    }

    /// Update a model.
    pub async fn update(&self, model: &Model) -> Result<(), CoreError> {
        self.repo.update(model).await.map_err(CoreError::from)
    }

    /// Delete a model by ID.
    pub async fn delete(&self, id: i64) -> Result<(), CoreError> {
        self.repo.delete(id).await.map_err(CoreError::from)
    }

    /// Remove a model by identifier. Returns the removed model.
    pub async fn remove(&self, identifier: &str) -> Result<Model, CoreError> {
        let model = self.find_by_identifier(identifier).await?;
        self.repo.delete(model.id).await.map_err(CoreError::from)?;
        Ok(model)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tag Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// List all unique tags used across all models.
    pub async fn list_tags(&self) -> Result<Vec<String>, CoreError> {
        let models = self.repo.list().await.map_err(CoreError::from)?;
        let mut all_tags = std::collections::HashSet::new();
        for model in models {
            for tag in model.tags {
                all_tags.insert(tag);
            }
        }
        let mut tags: Vec<String> = all_tags.into_iter().collect();
        tags.sort();
        Ok(tags)
    }

    /// Add a tag to a model.
    ///
    /// If the tag already exists on the model, this is a no-op.
    pub async fn add_tag(&self, model_id: i64, tag: String) -> Result<(), CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        if !model.tags.contains(&tag) {
            model.tags.push(tag);
            model.tags.sort();
            self.repo.update(&model).await.map_err(CoreError::from)?;
        }
        Ok(())
    }

    /// Remove a tag from a model.
    ///
    /// If the tag doesn't exist on the model, this is a no-op. System tags
    /// (see [`crate::domain::is_system_tag`]) are protected and cannot be
    /// removed through this API — use [`Self::remove_tag_force`] for
    /// admin/debug paths that intentionally need to drop them.
    pub async fn remove_tag(&self, model_id: i64, tag: &str) -> Result<(), CoreError> {
        if crate::domain::is_system_tag(tag) {
            return Err(CoreError::Validation(format!(
                "tag '{tag}' is a system tag and cannot be removed via the standard API",
            )));
        }
        self.remove_tag_force(model_id, tag).await
    }

    /// Force-remove a tag from a model, including system tags.
    ///
    /// Bypasses the system-tag protection enforced by [`Self::remove_tag`].
    /// Intended for admin/debug paths (e.g. the `gglib model retag --full`
    /// rebuild) where the caller intentionally needs to drop a `format:*`
    /// tag before re-detecting capabilities.
    pub async fn remove_tag_force(&self, model_id: i64, tag: &str) -> Result<(), CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        model.tags.retain(|t| t != tag);
        self.repo.update(&model).await.map_err(CoreError::from)?;
        Ok(())
    }

    /// Get all tags for a specific model.
    pub async fn get_tags(&self, model_id: i64) -> Result<Vec<String>, CoreError> {
        let model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        Ok(model.tags)
    }

    /// Get all models that have a specific tag.
    pub async fn get_by_tag(&self, tag: &str) -> Result<Vec<Model>, CoreError> {
        let models = self.repo.list().await.map_err(CoreError::from)?;
        Ok(models
            .into_iter()
            .filter(|m| m.tags.contains(&tag.to_string()))
            .collect())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Filter/Aggregate Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Get filter options aggregated from all models.
    ///
    /// Returns distinct quantizations, parameter count range, and context length range
    /// for use in the GUI filter popover.
    ///
    /// Note: Uses in-memory aggregation for simplicity. This is acceptable for typical
    /// model libraries (<100 models). Revisit if libraries grow large.
    pub async fn get_filter_options(&self) -> Result<crate::domain::ModelFilterOptions, CoreError> {
        use crate::domain::{ModelFilterOptions, RangeValues};
        use std::collections::HashSet;

        let models = self.repo.list().await.map_err(CoreError::from)?;

        // Collect distinct quantizations
        let mut quantizations: Vec<String> = models
            .iter()
            .filter_map(|m| m.quantization.clone())
            .filter(|q| !q.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        quantizations.sort();

        // Compute param_count_b range
        let param_range = if models.is_empty() {
            None
        } else {
            let min = models
                .iter()
                .map(|m| m.param_count_b)
                .fold(f64::INFINITY, f64::min);
            let max = models
                .iter()
                .map(|m| m.param_count_b)
                .fold(f64::NEG_INFINITY, f64::max);
            if min.is_finite() && max.is_finite() {
                Some(RangeValues { min, max })
            } else {
                None
            }
        };

        // Compute context_length range (only models with context_length set)
        let context_lengths: Vec<u64> = models.iter().filter_map(|m| m.context_length).collect();
        #[allow(clippy::cast_precision_loss)]
        let context_range = if context_lengths.is_empty() {
            None
        } else {
            let min = *context_lengths.iter().min().unwrap() as f64;
            let max = *context_lengths.iter().max().unwrap() as f64;
            Some(RangeValues { min, max })
        };

        // Compute latest_tg_tps range across benchmarked models
        let tps_values: Vec<f64> = models
            .iter()
            .filter_map(|m| m.benchmark_summary.as_ref()?.latest_tg_tps)
            .collect();
        let speed_range = if tps_values.is_empty() {
            None
        } else {
            let min = tps_values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = tps_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if min.is_finite() && max.is_finite() {
                Some(RangeValues { min, max })
            } else {
                None
            }
        };

        Ok(ModelFilterOptions {
            quantizations,
            param_range,
            context_range,
            speed_range,
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Capability Bootstrap
    // ─────────────────────────────────────────────────────────────────────────

    /// Backfill capabilities for models that don't have them set.
    ///
    /// This runs on startup to handle models with unknown capabilities.
    /// Only infers if capabilities are empty (0/unknown).
    ///
    /// # INVARIANT
    ///
    /// Never overwrite explicitly-set capabilities. Only infer when unknown.
    pub async fn bootstrap_capabilities(&self) -> Result<(), CoreError> {
        use crate::domain::{capabilities_from_architecture, infer_from_chat_template};

        let models = self.repo.list().await.map_err(CoreError::from)?;

        for mut model in models {
            // Only infer if capabilities are unknown (empty)
            if model.capabilities.is_empty() {
                let template = model.metadata.get("tokenizer.chat_template");
                let name = model.metadata.get("general.name");
                let arch = model.metadata.get("general.architecture");
                let from_template = infer_from_chat_template(
                    template.map(String::as_str),
                    name.map(String::as_str),
                );
                let from_arch = capabilities_from_architecture(arch.map(String::as_str));
                model.capabilities = from_template | from_arch;
                self.repo.update(&model).await.map_err(CoreError::from)?;
            }
        }

        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Retag
    // ─────────────────────────────────────────────────────────────────────────

    /// Re-derive auto-tags for a single model from its persisted GGUF metadata.
    ///
    /// `full = false` (default) is **additive**: any newly-detected tag that
    /// isn't already present is appended; nothing is ever removed. This is
    /// the safe path for backfilling `format:*` tags on models imported
    /// before format-tag detection landed.
    ///
    /// `full = true` performs a full rebuild: every previously auto-generated
    /// tag (the predefined capability tag namespace plus every existing
    /// `format:*` tag) is dropped and the freshly-detected set is added in
    /// its place. User-curated tags outside that namespace are preserved.
    ///
    /// Returns `None` when the tag set is unchanged (no write occurred) and
    /// `Some(diff)` when the model was updated, carrying the full added/removed
    /// delta.
    pub async fn retag_model(
        &self,
        model_id: i64,
        gguf_parser: &dyn GgufParserPort,
        full: bool,
    ) -> Result<Option<RetagDiff>, CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;

        // Re-derive capabilities from the persisted metadata blob; the file
        // doesn't have to exist on disk.
        let gguf_metadata = crate::domain::gguf::GgufMetadata {
            metadata: model.metadata.clone(),
            ..Default::default()
        };
        let new_tags = gguf_parser.detect_capabilities(&gguf_metadata).to_tags();

        let before: std::collections::BTreeSet<String> = model.tags.iter().cloned().collect();

        if full {
            // Drop every tag in the auto-generated namespace, then re-add.
            const AUTO_TAG_NAMES: &[&str] = &["reasoning", "agent", "vision", "code", "moe"];
            model.tags.retain(|t| {
                !AUTO_TAG_NAMES.contains(&t.as_str()) && !crate::domain::is_system_tag(t)
            });
        }

        for t in &new_tags {
            if !model.tags.contains(t) {
                model.tags.push(t.clone());
            }
        }
        model.tags.sort();

        let after: std::collections::BTreeSet<String> = model.tags.iter().cloned().collect();
        if after == before {
            return Ok(None);
        }

        self.repo.update(&model).await.map_err(CoreError::from)?;
        Ok(Some(RetagDiff {
            added: after.difference(&before).cloned().collect(),
            removed: before.difference(&after).cloned().collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ModelRepository, RepositoryError};
    use async_trait::async_trait;
    use chrono::Utc;

    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockRepo {
        models: Mutex<Vec<Model>>,
    }

    impl MockRepo {
        fn new() -> Self {
            Self {
                models: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl ModelRepository for MockRepo {
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

        #[allow(clippy::cast_possible_wrap, clippy::significant_drop_tightening)]
        async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
            let mut models = self.models.lock().unwrap();
            let id = models.len() as i64 + 1;
            let created = Model {
                id,
                name: model.name.clone(),
                file_path: model.file_path.clone(),
                param_count_b: model.param_count_b,
                architecture: model.architecture.clone(),
                quantization: model.quantization.clone(),
                context_length: model.context_length,
                expert_count: model.expert_count,
                expert_used_count: model.expert_used_count,
                expert_shared_count: model.expert_shared_count,
                metadata: model.metadata.clone(),
                added_at: model.added_at,
                hf_repo_id: model.hf_repo_id.clone(),
                hf_commit_sha: model.hf_commit_sha.clone(),
                hf_filename: model.hf_filename.clone(),
                download_date: model.download_date,
                last_update_check: model.last_update_check,
                tags: model.tags.clone(),
                capabilities: model.capabilities,
                inference_defaults: model.inference_defaults.clone(),
                benchmark_summary: None,
            };
            models.push(created.clone());
            Ok(created)
        }

        async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            models.iter_mut().find(|m| m.id == model.id).map_or_else(
                || Err(RepositoryError::NotFound(format!("id={}", model.id))),
                |m| {
                    m.clone_from(model);
                    Ok(())
                },
            )
        }

        async fn delete(&self, id: i64) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            let len_before = models.len();
            models.retain(|m| m.id != id);
            if models.len() == len_before {
                Err(RepositoryError::NotFound(format!("id={id}")))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn test_list_empty() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);
        let models = service.list().await.unwrap();
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_add_and_get() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let new_model = NewModel::new(
            "test-model".to_string(),
            PathBuf::from("/path/to/model.gguf"),
            7.0,
            Utc::now(),
        );

        let created = service.add(new_model).await.unwrap();
        assert_eq!(created.name, "test-model");

        let found = service.get_by_name("test-model").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_find_by_identifier_not_found() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let result = service.find_by_identifier("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_filter_options_empty() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let options = service.get_filter_options().await.unwrap();
        assert!(options.quantizations.is_empty());
        assert!(options.param_range.is_none());
        assert!(options.context_range.is_none());
    }

    #[tokio::test]
    async fn test_get_filter_options_with_models() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        // Add models with different characteristics
        let mut model1 = NewModel::new(
            "model-1".to_string(),
            PathBuf::from("/path/to/model1.gguf"),
            7.0,
            Utc::now(),
        );
        model1.quantization = Some("Q4_K_M".to_string());
        model1.context_length = Some(4096);

        let mut model2 = NewModel::new(
            "model-2".to_string(),
            PathBuf::from("/path/to/model2.gguf"),
            13.0,
            Utc::now(),
        );
        model2.quantization = Some("Q8_0".to_string());
        model2.context_length = Some(8192);

        let mut model3 = NewModel::new(
            "model-3".to_string(),
            PathBuf::from("/path/to/model3.gguf"),
            70.0,
            Utc::now(),
        );
        model3.quantization = Some("Q4_K_M".to_string()); // Duplicate quant
        // No context_length set

        service.add(model1).await.unwrap();
        service.add(model2).await.unwrap();
        service.add(model3).await.unwrap();

        let options = service.get_filter_options().await.unwrap();

        // Should have 2 distinct quantizations, sorted
        assert_eq!(options.quantizations, vec!["Q4_K_M", "Q8_0"]);

        // Param range: 7.0 to 70.0
        let param_range = options.param_range.unwrap();
        assert!((param_range.min - 7.0).abs() < 0.001);
        assert!((param_range.max - 70.0).abs() < 0.001);

        // Context range: 4096 to 8192 (model3 has no context)
        let context_range = options.context_range.unwrap();
        assert!((context_range.min - 4096.0).abs() < 0.001);
        assert!((context_range.max - 8192.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_remove_tag_rejects_system_tag() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model = NewModel::new(
            "qwen-test".to_string(),
            PathBuf::from("/path/to/m.gguf"),
            7.0,
            Utc::now(),
        );
        new_model.tags = vec!["chat".to_string(), "format:qwen-xml".to_string()];
        let created = service.add(new_model).await.unwrap();

        // Standard removal rejected.
        let err = service
            .remove_tag(created.id, "format:qwen-xml")
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));

        // Tag still present.
        let tags = service.get_tags(created.id).await.unwrap();
        assert!(tags.contains(&"format:qwen-xml".to_string()));

        // Force variant succeeds.
        service
            .remove_tag_force(created.id, "format:qwen-xml")
            .await
            .unwrap();
        let tags = service.get_tags(created.id).await.unwrap();
        assert!(!tags.contains(&"format:qwen-xml".to_string()));
    }

    #[tokio::test]
    async fn test_remove_tag_allows_user_tag() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model =
            NewModel::new("u".to_string(), PathBuf::from("/p.gguf"), 7.0, Utc::now());
        new_model.tags = vec!["chat".to_string(), "format:hermes".to_string()];
        let created = service.add(new_model).await.unwrap();

        service.remove_tag(created.id, "chat").await.unwrap();
        let tags = service.get_tags(created.id).await.unwrap();
        assert_eq!(tags, vec!["format:hermes".to_string()]);
    }

    /// Stub parser that emits a fixed capability set for retag tests.
    struct StubCapsParser {
        tags: Vec<String>,
    }

    impl crate::ports::GgufParserPort for StubCapsParser {
        fn parse(
            &self,
            _file_path: &std::path::Path,
        ) -> std::result::Result<crate::ports::GgufMetadata, crate::ports::GgufParseError> {
            Ok(crate::ports::GgufMetadata::default())
        }

        fn detect_capabilities(
            &self,
            _metadata: &crate::ports::GgufMetadata,
        ) -> crate::ports::GgufCapabilities {
            let mut extensions = std::collections::BTreeSet::new();
            for t in &self.tags {
                extensions.insert(t.clone());
            }
            crate::ports::GgufCapabilities {
                flags: crate::domain::gguf::CapabilityFlags::empty(),
                extensions,
            }
        }
    }

    #[tokio::test]
    async fn test_retag_additive_appends_missing_tags() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model =
            NewModel::new("m".to_string(), PathBuf::from("/p.gguf"), 7.0, Utc::now());
        new_model.tags = vec!["chat".to_string()];
        let created = service.add(new_model).await.unwrap();

        let parser = StubCapsParser {
            tags: vec!["format:qwen-xml".to_string()],
        };
        let diff = service
            .retag_model(created.id, &parser, false)
            .await
            .unwrap();
        assert_eq!(diff.unwrap().added, vec!["format:qwen-xml".to_string()]);

        let tags = service.get_tags(created.id).await.unwrap();
        assert!(tags.contains(&"chat".to_string()));
        assert!(tags.contains(&"format:qwen-xml".to_string()));
    }

    #[tokio::test]
    async fn test_retag_additive_noop_when_already_present() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model =
            NewModel::new("m".to_string(), PathBuf::from("/p.gguf"), 7.0, Utc::now());
        new_model.tags = vec!["format:qwen-xml".to_string()];
        let created = service.add(new_model).await.unwrap();

        let parser = StubCapsParser {
            tags: vec!["format:qwen-xml".to_string()],
        };
        let diff = service
            .retag_model(created.id, &parser, false)
            .await
            .unwrap();
        assert!(diff.is_none());
    }

    #[tokio::test]
    async fn test_retag_full_replaces_auto_tags_preserves_user() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model =
            NewModel::new("m".to_string(), PathBuf::from("/p.gguf"), 7.0, Utc::now());
        new_model.tags = vec![
            "favorite".to_string(),      // user
            "format:hermes".to_string(), // stale auto
            "reasoning".to_string(),     // stale auto capability
        ];
        let created = service.add(new_model).await.unwrap();

        let parser = StubCapsParser {
            tags: vec!["format:qwen-xml".to_string()],
        };
        service
            .retag_model(created.id, &parser, true)
            .await
            .unwrap();

        let tags = service.get_tags(created.id).await.unwrap();
        assert!(tags.contains(&"favorite".to_string()));
        assert!(tags.contains(&"format:qwen-xml".to_string()));
        assert!(!tags.contains(&"format:hermes".to_string()));
        assert!(!tags.contains(&"reasoning".to_string()));
    }
}
