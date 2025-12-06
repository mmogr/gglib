//! Model service for GGUF model CRUD operations.
//!
//! This service provides all model-related operations extracted from
//! the GUI backend, designed to be used by both CLI and GUI.

use crate::commands::download::get_models_directory;
use crate::download::DownloadId;
use crate::models::Gguf;
use crate::services::database;
use anyhow::{Result, anyhow};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Service for managing GGUF models in the database.
///
/// Provides pure CRUD operations without interactive prompts.
/// CLI commands handle user interaction, then call these methods
/// with complete data.
#[derive(Clone)]
pub struct ModelService {
    db_pool: SqlitePool,
}

impl ModelService {
    /// Create a new ModelService with the given database pool.
    pub fn new(db_pool: SqlitePool) -> Self {
        Self { db_pool }
    }

    /// List all models in the database.
    ///
    /// Returns models sorted by name. Does not include serving status —
    /// use `GuiBackend.list_models()` for GUI with server status.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use gglib::services::core::ModelService;
    /// # async fn example(service: &ModelService) -> anyhow::Result<()> {
    /// let models = service.list().await?;
    /// for model in models {
    ///     println!("{}: {} ({}B params)",
    ///         model.id.unwrap_or(0),
    ///         model.name,
    ///         model.param_count_b
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(&self) -> Result<Vec<Gguf>> {
        database::list_models(&self.db_pool).await
    }

    /// Get a model by its numeric ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The database ID of the model
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not found.
    pub async fn get_by_id(&self, id: u32) -> Result<Gguf> {
        database::get_model_by_id(&self.db_pool, id)
            .await?
            .ok_or_else(|| anyhow!("Model with ID {} not found", id))
    }

    /// Find a model by identifier (ID or name).
    ///
    /// First tries to parse as numeric ID, then searches by exact name match.
    ///
    /// # Arguments
    ///
    /// * `identifier` - Either a numeric ID string or model name
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(model))` if found, `Ok(None)` if not found.
    pub async fn find_by_identifier(&self, identifier: &str) -> Result<Option<Gguf>> {
        database::find_model_by_identifier(&self.db_pool, identifier).await
    }

    /// Find models by partial name match.
    ///
    /// Useful for fuzzy searching when exact match fails.
    ///
    /// # Arguments
    ///
    /// * `name` - Partial name to search for (case-insensitive)
    pub async fn find_by_name(&self, name: &str) -> Result<Vec<Gguf>> {
        database::find_models_by_name(&self.db_pool, name).await
    }

    /// Find a model by exact name with case-insensitive matching.
    ///
    /// Unlike `find_by_identifier`, this performs case-insensitive exact match
    /// on the name only (no ID fallback). Used by ProcessManager for SingleSwap
    /// strategy when resolving model names from OpenAI API requests.
    ///
    /// # Arguments
    ///
    /// * `name` - Model name to search for (case-insensitive exact match)
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(model))` if found, `Ok(None)` if not found.
    pub async fn find_by_name_case_insensitive(&self, name: &str) -> Result<Option<Gguf>> {
        database::find_model_by_name_case_insensitive(&self.db_pool, name).await
    }

    /// Find a model by its file path.
    ///
    /// Used for idempotency checks before adding downloaded models.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Exact file path to search for
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(model))` if found, `Ok(None)` if not found.
    pub async fn find_by_path(&self, file_path: &str) -> Result<Option<Gguf>> {
        database::find_model_by_path(&self.db_pool, file_path).await
    }

    /// Add a new model to the database.
    ///
    /// The model should have all fields populated except `id`, which
    /// will be assigned by the database.
    ///
    /// # Arguments
    ///
    /// * `model` - The model to add (id field will be ignored)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A model with the same file path already exists
    /// - Database operation fails
    pub async fn add(&self, model: &Gguf) -> Result<()> {
        database::add_model(&self.db_pool, model).await
    }

    /// Add a model from a GGUF file path with automatic metadata extraction.
    ///
    /// This is a convenience method that:
    /// 1. Validates the file exists and is a GGUF file
    /// 2. Extracts metadata from the GGUF header
    /// 3. Creates and saves the model to the database
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the GGUF file
    /// * `name_override` - Optional name to use instead of extracted name
    /// * `param_count_override` - Optional parameter count to use
    ///
    /// # Returns
    ///
    /// Returns the newly created model with its database ID.
    pub async fn add_from_file(
        &self,
        file_path: &str,
        name_override: Option<String>,
        param_count_override: Option<f64>,
    ) -> Result<Gguf> {
        use crate::utils::validation;

        let path = PathBuf::from(file_path);

        // Validate file exists
        if !path.exists() {
            return Err(anyhow!("File not found: {}", file_path));
        }

        // Validate it's a GGUF file
        if !file_path.to_lowercase().ends_with(".gguf") {
            return Err(anyhow!("File must have .gguf extension"));
        }

        // Extract metadata from GGUF file
        let gguf_metadata = validation::validate_and_parse_gguf(file_path)?;

        // Determine name: override > extracted > filename
        let name = name_override.or(gguf_metadata.name).unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown Model")
                .to_string()
        });

        // Determine param count: override > extracted > 0
        let param_count_b = param_count_override
            .or(gguf_metadata.param_count_b)
            .unwrap_or(0.0);

        // Auto-detect reasoning and tool calling capabilities from metadata
        let tags = crate::gguf::apply_capability_detection(&gguf_metadata.metadata);

        // Create the model instance
        let new_model = Gguf {
            id: None,
            name,
            file_path: path.clone(),
            param_count_b,
            architecture: gguf_metadata.architecture,
            quantization: gguf_metadata.quantization,
            context_length: gguf_metadata.context_length,
            metadata: gguf_metadata.metadata,
            added_at: chrono::Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags,
        };

        // Save to database
        database::add_model(&self.db_pool, &new_model).await?;

        // Retrieve the newly added model (to get the ID)
        let models = database::list_models(&self.db_pool).await?;
        models
            .into_iter()
            .find(|m| m.file_path == path)
            .ok_or_else(|| anyhow!("Model was added but could not be retrieved"))
    }

    /// Update an existing model in the database.
    ///
    /// # Arguments
    ///
    /// * `id` - The database ID of the model to update
    /// * `model` - The updated model data
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not found.
    pub async fn update(&self, id: u32, model: &Gguf) -> Result<()> {
        database::update_model(&self.db_pool, id, model).await
    }

    /// Remove a model from the database by ID.
    ///
    /// This only removes the database entry; the actual file remains on disk.
    ///
    /// # Arguments
    ///
    /// * `id` - The database ID of the model to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not found.
    pub async fn remove(&self, id: u32) -> Result<()> {
        database::remove_model_by_id(&self.db_pool, id).await
    }

    // Tag Management Operations

    /// List all unique tags used across all models.
    pub async fn list_tags(&self) -> Result<Vec<String>> {
        database::list_tags(&self.db_pool).await
    }

    /// Add a tag to a model.
    ///
    /// If the tag already exists on the model, this is a no-op.
    pub async fn add_tag(&self, model_id: u32, tag: String) -> Result<()> {
        database::add_model_tag(&self.db_pool, model_id, tag).await
    }

    /// Remove a tag from a model.
    pub async fn remove_tag(&self, model_id: u32, tag: String) -> Result<()> {
        database::remove_model_tag(&self.db_pool, model_id, tag).await
    }

    /// Get all tags for a specific model.
    pub async fn get_tags(&self, model_id: u32) -> Result<Vec<String>> {
        database::get_model_tags(&self.db_pool, model_id).await
    }

    /// Get all model IDs that have a specific tag.
    pub async fn get_by_tag(&self, tag: &str) -> Result<Vec<u32>> {
        database::get_models_by_tag(&self.db_pool, tag.to_string()).await
    }

    /// Get filter options for the model library UI.
    ///
    /// Returns aggregate data about available models for building
    /// dynamic filter controls (quantizations, param range, context range).
    pub async fn get_filter_options(&self) -> Result<database::ModelFilterOptions> {
        database::get_model_filter_options(&self.db_pool).await
    }

    /// Resolve the path to the downloaded GGUF file for a download.
    ///
    /// Looks in the model directory for the first `.gguf` file.
    /// Used by the completion handler to register downloads in the database.
    ///
    /// # Arguments
    ///
    /// * `download_id` - The download identifier containing model_id
    ///
    /// # Returns
    ///
    /// Returns the path to the GGUF file, or an error if not found.
    pub fn model_path_for_download(download_id: &DownloadId) -> Result<PathBuf> {
        let models_dir = get_models_directory()?;
        let model_dir = models_dir.join(sanitize_model_name(download_id.model_id()));

        // Find first .gguf file in the directory
        let entries = std::fs::read_dir(&model_dir)
            .map_err(|e| anyhow!("Failed to read model directory {:?}: {}", model_dir, e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "gguf") {
                return Ok(path);
            }
        }

        Err(anyhow!(
            "No .gguf file found in model directory {:?}",
            model_dir
        ))
    }
}

/// Sanitize model name for filesystem use.
fn sanitize_model_name(model_id: &str) -> String {
    model_id.replace(['/', '\\'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;

    #[tokio::test]
    async fn test_model_service_list() {
        let pool = database::setup_test_database().await.unwrap();
        let service = ModelService::new(pool);

        // Should not panic, returns empty or existing models
        let result = service.list().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_model_service_get_nonexistent() {
        let pool = database::setup_test_database().await.unwrap();
        let service = ModelService::new(pool);

        // Should return error for non-existent ID
        let result = service.get_by_id(999999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_model_service_find_by_identifier_nonexistent() {
        let pool = database::setup_test_database().await.unwrap();
        let service = ModelService::new(pool);

        // Should return None for non-existent identifier
        let result = service.find_by_identifier("nonexistent-model-xyz").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
