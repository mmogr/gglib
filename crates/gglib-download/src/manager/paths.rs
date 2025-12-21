//! Destination path planning for downloads.
//!
//! This module handles the planning and creation of download destinations,
//! including model directories and temporary file management.

use std::path::{Path, PathBuf};

use gglib_core::download::{DownloadError, DownloadId};

/// A planned download destination.
#[derive(Debug, Clone)]
pub struct DownloadDestination {
    /// The model directory where files will be stored.
    pub model_dir: PathBuf,
    /// The files to download (relative paths within the model dir).
    pub files: Vec<String>,
}

impl DownloadDestination {
    /// Create a new download destination plan.
    ///
    /// # Arguments
    ///
    /// * `models_directory` - Base directory for all models
    /// * `id` - Download ID used to derive the subdirectory name
    /// * `files` - List of files to download
    pub fn plan(models_directory: &Path, id: &DownloadId, files: Vec<String>) -> Self {
        // Convert repo ID to a safe directory name (replace / with _)
        let dir_name = id.model_id().replace('/', "_");
        let model_dir = models_directory.join(dir_name);

        Self { model_dir, files }
    }

    /// Ensure the model directory exists, creating it if necessary.
    pub fn ensure_dir(&self) -> Result<(), DownloadError> {
        if !self.model_dir.exists() {
            std::fs::create_dir_all(&self.model_dir)
                .map_err(|e| DownloadError::io("create_dir", e.to_string()))?;
        }
        Ok(())
    }

    /// Get the primary file path (first file in the list).
    pub fn primary_path(&self) -> Option<PathBuf> {
        self.files.first().map(|f| self.model_dir.join(f))
    }

    /// Get all file paths.
    pub fn all_paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|f| self.model_dir.join(f)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_creates_correct_model_dir() {
        let base = PathBuf::from("/models");
        let id = DownloadId::new("unsloth/Llama-3-GGUF", Some("Q4_K_M"));
        let files = vec!["model.gguf".to_string()];

        let dest = DownloadDestination::plan(&base, &id, files);

        assert_eq!(
            dest.model_dir,
            PathBuf::from("/models/unsloth_Llama-3-GGUF")
        );
        assert_eq!(dest.files, vec!["model.gguf"]);
    }

    #[test]
    fn primary_path_returns_first_file() {
        let base = PathBuf::from("/models");
        let id = DownloadId::new("test/model", Some("Q4"));
        let files = vec!["file1.gguf".to_string(), "file2.gguf".to_string()];

        let dest = DownloadDestination::plan(&base, &id, files);

        assert_eq!(
            dest.primary_path(),
            Some(PathBuf::from("/models/test_model/file1.gguf"))
        );
    }

    #[test]
    fn all_paths_returns_full_paths() {
        let base = PathBuf::from("/models");
        let id = DownloadId::new("test/model", Some("Q4"));
        let files = vec!["file1.gguf".to_string(), "file2.gguf".to_string()];

        let dest = DownloadDestination::plan(&base, &id, files);
        let paths = dest.all_paths();

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/models/test_model/file1.gguf"));
        assert_eq!(paths[1], PathBuf::from("/models/test_model/file2.gguf"));
    }
}
