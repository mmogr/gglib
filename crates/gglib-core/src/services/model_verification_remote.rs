//! The two verification operations that ask the repository: the update
//! check, and repair.
//!
//! A model's files are its weights and, when it was downloaded with one, a
//! projector. Both operations cover both: the update check compares the
//! projector's OID with the repository's, and a repair deletes an unhealthy
//! projector only when the download it queues brings that file back.

use std::path::Path;

use tokio::sync::mpsc;

use super::model_verification::{
    ModelVerificationService, OperationType, ShardHealth, ShardUpdate, UpdateCheckResult,
    UpdateDetails,
};
use crate::domain::ModelFile;
use crate::download::{GgufFileRole, Quantization};
use crate::ports::RepositoryError;
use crate::ports::huggingface::download_group;

/// Whether a model's file row is its projector's.
fn is_projector(file: &ModelFile) -> bool {
    GgufFileRole::classify(Path::new(&file.file_path)).is_projector()
}

impl ModelVerificationService {
    /// Check if updates are available for a model.
    ///
    /// Compares local OIDs with remote OIDs from `HuggingFace`, for the
    /// weights and for a projector downloaded with them.
    pub async fn check_for_updates(
        &self,
        model_id: i64,
    ) -> Result<UpdateCheckResult, RepositoryError> {
        let unchanged = UpdateCheckResult {
            model_id,
            update_available: false,
            details: None,
        };

        // Get model metadata
        let model = self.model_repo.get_by_id(model_id).await?;

        let (Some(repo_id), Some(quantization)) = (&model.hf_repo_id, &model.quantization) else {
            return Ok(unchanged);
        };

        // Get local file metadata
        let local_files = self.files_of(model_id).await?;

        if local_files.is_empty() {
            return Ok(unchanged);
        }

        // Get remote file metadata from HuggingFace. The quantization's files
        // are weights alone, so a model with a projector row also asks for
        // the repository's projectors.
        let fetch_failed =
            |e| RepositoryError::Storage(format!("Failed to fetch remote files: {e}"));
        let mut remote_files = self
            .hf_client
            .get_quantization_files(repo_id, quantization)
            .await
            .map_err(fetch_failed)?;
        if local_files.iter().any(is_projector) {
            let projectors = self.hf_client.list_projectors(repo_id).await;
            remote_files.extend(projectors.map_err(fetch_failed)?);
        }

        // Compare OIDs
        let mut changes = Vec::new();

        for local_file in &local_files {
            let Some(ref local_oid) = local_file.hf_oid else {
                continue;
            };

            // Find matching remote file by path
            if let Some(remote_file) = remote_files.iter().find(|f| f.path == local_file.file_path)
                && let Some(ref remote_oid) = remote_file.oid
                && local_oid != remote_oid
            {
                #[allow(clippy::cast_sign_loss)]
                let index = local_file.file_index as usize;
                changes.push(ShardUpdate {
                    index,
                    file_path: local_file.file_path.clone(),
                    old_oid: local_oid.clone(),
                    new_oid: remote_oid.clone(),
                });
            }
        }

        if changes.is_empty() {
            return Ok(unchanged);
        }
        Ok(UpdateCheckResult {
            model_id,
            update_available: true,
            details: Some(UpdateDetails {
                changed_shards: changes.len(),
                changes,
            }),
        })
    }

    /// Repair a model by re-downloading corrupt or missing files.
    ///
    /// The unhealthy files are deleted and the model's download is queued
    /// again, which fetches what is missing. An unhealthy projector is
    /// deleted only when that download fetches it; one it would not bring
    /// back stays on disk and stays linked.
    ///
    /// # Arguments
    ///
    /// * `model_id` - ID of the model to repair
    /// * `shard_indices` - Optional list of specific file indices to repair.
    ///   If `None`, all unhealthy files will be repaired.
    ///
    /// # Errors
    ///
    /// A message when the model has no repository or quantization, when
    /// nothing is unhealthy, when every unhealthy file is a projector the
    /// download would not bring back, and when the repository cannot be read
    /// or the download cannot be queued.
    pub async fn repair_model(
        &self,
        model_id: i64,
        shard_indices: Option<Vec<usize>>,
    ) -> Result<String, String> {
        // Acquire downloading lock
        let _guard = self
            .operation_lock
            .try_acquire(model_id, OperationType::Downloading)
            .await?;

        // Get model metadata
        let model = self
            .model_repo
            .get_by_id(model_id)
            .await
            .map_err(|e| format!("Failed to get model: {e}"))?;

        let Some(ref repo_id) = model.hf_repo_id else {
            return Err("Model does not have HuggingFace repository information".to_string());
        };

        let Some(ref quantization) = model.quantization else {
            return Err("Model does not have quantization information".to_string());
        };

        // Get file metadata
        let model_files = self
            .files_of(model_id)
            .await
            .map_err(|e| format!("Failed to get model files: {e}"))?;

        // Get base directory from model's file path
        let base_dir = model
            .file_path
            .parent()
            .ok_or_else(|| "Failed to get model directory".to_string())?
            .to_path_buf();

        // Determine which files to repair
        let mut to_repair: Vec<&ModelFile> = if let Some(indices) = shard_indices {
            #[allow(clippy::cast_sign_loss)]
            let filter_fn = |f: &&ModelFile| indices.contains(&(f.file_index as usize));
            model_files.iter().filter(filter_fn).collect()
        } else {
            // Verify all files to find unhealthy ones
            let mut unhealthy = Vec::new();
            for file in &model_files {
                // Nobody reads this progress. The receiver is dropped here so
                // each send fails at once; kept alive, the channel fills
                // after one message and the hashing thread waits forever.
                let (tx, _) = mpsc::channel(1);
                let resolved_path = base_dir.join(&file.file_path);
                let health = Self::verify_shard(file, &resolved_path, model_id, 0, 1, &tx).await;
                if matches!(health, ShardHealth::Corrupt { .. } | ShardHealth::Missing) {
                    unhealthy.push(file);
                }
            }
            unhealthy
        };

        if to_repair.is_empty() {
            return Err("No unhealthy shards found to repair".to_string());
        }

        // A projector is deleted only when the download queued below fetches
        // that same file. Each one is judged on its own.
        if to_repair.iter().any(|file| is_projector(file)) {
            let fetched = self.fetched_projector(repo_id, quantization).await?;
            let (repaired, left): (Vec<&ModelFile>, Vec<&ModelFile>) =
                to_repair.into_iter().partition(|file| {
                    !is_projector(file) || fetched.as_deref() == Some(file.file_path.as_str())
                });
            if repaired.is_empty() {
                let names: Vec<&str> = left.iter().map(|file| file.file_path.as_str()).collect();
                return Err(format!(
                    "{} is unhealthy, and a download of {repo_id} at {quantization} does \
                     not fetch it, so it was left in place. Link another projector with \
                     `gglib model update {model_id} --projector <path>`",
                    names.join(", ")
                ));
            }
            to_repair = repaired;
        }

        // Delete corrupt/missing files
        for file in &to_repair {
            let resolved_path = base_dir.join(&file.file_path);
            if resolved_path.exists()
                && let Err(e) = tokio::fs::remove_file(&resolved_path).await
            {
                tracing::warn!(
                    model_id = model_id,
                    file_path = %file.file_path,
                    error = %e,
                    "Failed to delete corrupt file"
                );
            }
        }

        // Trigger re-download
        let download_id = self
            .download_trigger
            .queue_download(repo_id.clone(), Some(quantization.clone()))
            .await
            .map_err(|e| format!("Failed to queue download: {e}"))?;

        Ok(download_id)
    }

    /// The path of the projector a download of `quantization` from `repo_id`
    /// fetches, when it fetches one.
    async fn fetched_projector(
        &self,
        repo_id: &str,
        quantization: &str,
    ) -> Result<Option<String>, String> {
        let quantization: Quantization = quantization
            .parse()
            .map_err(|()| format!("{quantization} is not a quantization a download asks for"))?;
        let group = download_group(self.hf_client.as_ref(), repo_id, quantization)
            .await
            .map_err(|e| format!("Failed to read what a download of {repo_id} fetches: {e}"))?;
        Ok(group.projector.map(|projector| projector.path))
    }
}

#[cfg(test)]
#[path = "model_verification_remote_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "model_verification_two_projector_tests.rs"]
mod two_projector_tests;

#[cfg(test)]
#[path = "model_verification_store_tests.rs"]
mod store_tests;
