//! Destination path planning for downloads.
//!
//! This module handles the planning and creation of download destinations:
//! the models directory a download goes under, and its model's folder there.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{MutexGuard, PoisonError};

use gglib_core::download::{DownloadError, DownloadId};
use gglib_core::paths::{repository_dir, resolve_models_dir};

use super::DownloadManagerImpl;
use crate::queue::QueuedItem;

impl DownloadManagerImpl {
    /// The models directory of each download that has started. A std mutex:
    /// it is taken for a moment, with no other std mutex held, and never
    /// across an await.
    pub(super) fn directories(&self) -> MutexGuard<'_, HashMap<DownloadId, PathBuf>> {
        self.directories
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The models directory a download starting now goes under: the one the
    /// config names, or with none the one [`resolve_models_dir`] answers now.
    fn current_models_directory(&self) -> Result<PathBuf, DownloadError> {
        if let Some(named) = &self.config.models_directory {
            return Ok(named.clone());
        }
        let resolved = resolve_models_dir(None).map_err(|e| {
            DownloadError::other(format!("Could not resolve the models directory: {e}"))
        })?;
        Ok(resolved.path)
    }

    /// Where `item`'s file goes: in the folder of the repository it is
    /// fetched from, under the models directory its download started with.
    /// That is its model's folder, except for an image model's companion,
    /// which goes in its own repository's folder: one place for the file,
    /// whichever model's download fetches it, so the next finds it there.
    ///
    /// A download's first file takes the directory current as it starts,
    /// and the files after it take the same one, until the download ends
    /// and `end_download` drops it. So the next download, the same one
    /// queued again included, goes where the directory resolves then, and a
    /// download part fetched is not split across two.
    pub(super) fn destination(
        &self,
        item: &QueuedItem,
    ) -> Result<DownloadDestination, DownloadError> {
        let started_under = self.directories().get(&item.id).cloned();
        let models_directory = if let Some(kept) = started_under {
            kept
        } else {
            let current = self.current_models_directory()?;
            self.directories().insert(item.id.clone(), current.clone());
            current
        };
        let files = Self::extract_files(item);
        Ok(DownloadDestination::plan(
            &models_directory,
            item.repo(),
            files,
        ))
    }
}

/// A planned download destination.
#[derive(Debug, Clone)]
pub(crate) struct DownloadDestination {
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
    /// * `repo_id` - The repository the files come from, which the
    ///   subdirectory name derives from
    /// * `files` - List of files to download
    pub(crate) fn plan(models_directory: &Path, repo_id: &str, files: Vec<String>) -> Self {
        let model_dir = repository_dir(models_directory, repo_id);

        Self { model_dir, files }
    }

    /// Ensure the model directory exists, creating it if necessary.
    pub(crate) fn ensure_dir(&self) -> Result<(), DownloadError> {
        if !self.model_dir.exists() {
            std::fs::create_dir_all(&self.model_dir)
                .map_err(|e| DownloadError::io("create_dir", e.to_string()))?;
        }
        Ok(())
    }

    /// Get the primary file path (first file in the list).
    pub(crate) fn primary_path(&self) -> Option<PathBuf> {
        self.files.first().map(|f| self.model_dir.join(f))
    }

    /// Get all file paths.
    pub(crate) fn all_paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|f| self.model_dir.join(f)).collect()
    }
}

#[cfg(test)]
#[path = "models_directory_tests.rs"]
mod models_directory_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_creates_correct_model_dir() {
        let base = PathBuf::from("/models");
        let files = vec!["model.gguf".to_string()];

        let dest = DownloadDestination::plan(&base, "unsloth/Llama-3-GGUF", files);

        assert_eq!(
            dest.model_dir,
            PathBuf::from("/models/unsloth_Llama-3-GGUF")
        );
        assert_eq!(dest.files, vec!["model.gguf"]);
    }

    #[test]
    fn primary_path_returns_first_file() {
        let base = PathBuf::from("/models");
        let files = vec!["file1.gguf".to_string(), "file2.gguf".to_string()];

        let dest = DownloadDestination::plan(&base, "test/model", files);

        assert_eq!(
            dest.primary_path(),
            Some(PathBuf::from("/models/test_model/file1.gguf"))
        );
    }

    #[test]
    fn all_paths_returns_full_paths() {
        let base = PathBuf::from("/models");
        let files = vec!["file1.gguf".to_string(), "file2.gguf".to_string()];

        let dest = DownloadDestination::plan(&base, "test/model", files);
        let paths = dest.all_paths();

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/models/test_model/file1.gguf"));
        assert_eq!(paths[1], PathBuf::from("/models/test_model/file2.gguf"));
    }
}
