//! The two verification operations that ask the repository: the update
//! check, and repair. With the repair, what it answers with, and what is said
//! of its files when the download does not bring them back.
//!
//! A model's files are its weights and, when it was downloaded with one, a
//! projector. Both operations cover both: the update check compares the
//! projector's OID with the repository's, and a repair deletes an unhealthy
//! projector only when the download it queues brings that file back. An
//! image model's components are its files too, and a repair never deletes
//! one: the file lives in its own repository's folder, and other models may
//! draw with it.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::model_links::resolved_or_literal;
use super::model_row_roles::row_roles;
use super::model_verification::{
    FilePlace, ModelVerificationService, OperationType, ShardHealth, ShardUpdate,
    UpdateCheckResult, UpdateDetails, VerifiedFileKind,
};
use crate::domain::{Model, ModelFile, ModelFolder, NO_HUB_LABEL};
use crate::download::{DownloadId, GgufFileRole};
use crate::ports::RepositoryError;
use crate::ports::huggingface::{download_group, stored_label};

/// Whether `file` is a projector, among `rows` whose roles are `roles`, in
/// the same order.
fn is_projector(rows: &[ModelFile], roles: &[GgufFileRole], file: &ModelFile) -> bool {
    rows.iter()
        .zip(roles)
        .any(|(row, role)| row.file_path == file.file_path && role.is_projector())
}

/// The component `model` links to the file of `row`, read in `folder`, when
/// it links one there.
fn component_at(model: &Model, folder: &ModelFolder, row: &ModelFile) -> Option<String> {
    let path = resolved_or_literal(&folder.file(&row.file_path));
    model
        .components
        .iter()
        .find(|link| link.path == path)
        .map(|link| link.role.to_string())
}

/// The rows of `to_repair` that are not one of `model`'s component links,
/// read in `folder`.
///
/// A component is never deleted by a repair: it is in its own repository's
/// folder, and another model may draw with the same file.
///
/// # Errors
///
/// When every row is a component: the message names the files and their
/// roles, says they were left in place, and gives the command that links
/// another.
fn without_components<'a>(
    model: &Model,
    folder: &ModelFolder,
    to_repair: Vec<&'a ModelFile>,
) -> Result<Vec<&'a ModelFile>, String> {
    let (components, rest): (Vec<&ModelFile>, Vec<&ModelFile>) = to_repair
        .into_iter()
        .partition(|file| component_at(model, folder, file).is_some());
    if !rest.is_empty() || components.is_empty() {
        return Ok(rest);
    }
    let names: Vec<&str> = components
        .iter()
        .map(|file| file.file_path.as_str())
        .collect();
    let roles: Vec<String> = components
        .iter()
        .filter_map(|file| component_at(model, folder, file))
        .collect();
    let role = match roles.as_slice() {
        [one] => one.as_str(),
        _ => "<role>",
    };
    Err(format!(
        "{} is unhealthy, and it is the model's {} component, which a repair never deletes, \
         so it was left in place. Link another with `gglib model update {} --component \
         {role}=<path>`",
        names.join(", "),
        roles.join(", "),
        model.id
    ))
}

/// A repair under way: its unhealthy files are off the disk, and the
/// download that fetches them again is queued and started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RepairStarted {
    /// The download's canonical ID: the row to watch in the queue.
    pub id: String,
    /// The files the download is to bring back, as the model's rows name
    /// them. None of them is on disk when the repair answers.
    pub files: Vec<String>,
}

/// Why `model`'s files cannot be looked up in `repo_id`: it was stored with
/// no label, or with one the repository no longer has, and no one variant
/// holds its file.
fn no_label(repo_id: &str, model: &Model) -> String {
    model.variant_label.as_ref().map_or_else(
        || NO_HUB_LABEL.to_owned(),
        |label| {
            format!(
                "no variant of {repo_id} is labelled '{label}' any more, and none holds this \
                 model's file, so its files cannot be found on the Hub again; download it again \
                 to check, repair or upgrade it"
            )
        },
    )
}

/// What to say of `files`, missing from a model's folder since a repair, when
/// `download` did not bring them back: their names, and the command that
/// fetches them.
#[must_use]
pub fn missing_after_repair(download: &DownloadId, files: &[String]) -> String {
    let quantization = download
        .quantization()
        .map_or_else(String::new, |q| format!(" --quantization {q}"));
    format!(
        "Missing from the model's folder: {}. Run `gglib model download {}{quantization}` to \
         fetch what is missing.",
        files.join(", "),
        download.model_id()
    )
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

        let Some(repo_id) = &model.hf_repo_id else {
            return Ok(unchanged);
        };

        // Get local file metadata
        let local_files = self.files_of(model_id).await?;

        if local_files.is_empty() {
            return Ok(unchanged);
        }

        let fetch_failed =
            |e| RepositoryError::Storage(format!("Failed to fetch remote files: {e}"));
        // Not "unchanged": without its label the check cannot be made.
        let Some(label) = stored_label(self.hf_client.as_ref(), repo_id, &model)
            .await
            .map_err(fetch_failed)?
        else {
            return Err(RepositoryError::Constraint(no_label(repo_id, &model)));
        };

        // Get remote file metadata from HuggingFace. The variant's files
        // are weights alone, so a model with a projector row, by its header
        // or its link, also asks for the repository's files, its projector
        // found among them by path.
        let mut remote_files = self
            .hf_client
            .get_quantization_files(repo_id, &label)
            .await
            .map_err(fetch_failed)?;
        let folder = ModelFolder::of(&model.file_path, &local_files);
        let roles = row_roles(&model, &folder, &local_files, self.gguf_parser.as_ref());
        if roles.iter().any(|role| role.is_projector()) {
            let files = self.hf_client.list_gguf_files(repo_id).await;
            remote_files.extend(files.map_err(fetch_failed)?);
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
    /// again, at the quantization the model was stored with, which fetches
    /// what is missing. An unhealthy projector is deleted only when that
    /// download fetches it; one it would not bring back stays on disk and
    /// stays linked.
    ///
    /// What the download fetches is read from the repository before any file
    /// is deleted, so a repository that cannot be read, or that does not hold
    /// the stored quantization, costs the model nothing.
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
    /// nothing is unhealthy, when every unhealthy file is a component the
    /// model links (never deleted) or a projector the download would not
    /// bring back, when the repository cannot be read, and when no unhealthy
    /// file could be deleted: in each of those nothing was deleted. A
    /// download that cannot be queued after the files are gone is a message
    /// that names them, with [`missing_after_repair`]'s words.
    pub async fn repair_model(
        &self,
        model_id: i64,
        shard_indices: Option<Vec<usize>>,
    ) -> Result<RepairStarted, String> {
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

        // Get file metadata
        let model_files = self
            .files_of(model_id)
            .await
            .map_err(|e| format!("Failed to get model files: {e}"))?;

        let quantization = stored_label(self.hf_client.as_ref(), repo_id, &model)
            .await
            .map_err(|e| format!("Failed to list {repo_id}'s variants: {e}"))?
            .ok_or_else(|| no_label(repo_id, &model))?;

        // The folder the download wrote the files in, where each row is read.
        let folder = ModelFolder::of(&model.file_path, &model_files);

        // What each file is, read before anything is deleted.
        let roles = row_roles(&model, &folder, &model_files, self.gguf_parser.as_ref());

        // Determine which files to repair
        let to_repair: Vec<&ModelFile> = if let Some(indices) = shard_indices {
            #[allow(clippy::cast_sign_loss)]
            let filter_fn = |f: &&ModelFile| indices.contains(&(f.file_index as usize));
            model_files.iter().filter(filter_fn).collect()
        } else {
            let kinds = VerifiedFileKind::of_rows(&model_files, &roles);
            Self::unhealthy(model_id, &folder, &model_files, kinds).await
        };

        if to_repair.is_empty() {
            return Err("No unhealthy shards found to repair".to_string());
        }

        // A component the model links is never deleted, whatever else is
        // repaired.
        let mut to_repair = without_components(&model, &folder, to_repair)?;

        // What the download queued below fetches, asked while every file is
        // still in place.
        let fetched = self.fetched_projector(repo_id, &quantization).await?;

        // A projector is deleted only when that download fetches that same
        // file. Each one is judged on its own.
        let is_projector = |file: &ModelFile| is_projector(&model_files, &roles, file);
        if to_repair.iter().any(|file| is_projector(file)) {
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

        // Delete the unhealthy files. One that cannot be deleted is not
        // fetched again, since a download takes a file on disk as it is.
        let mut gone = Vec::new();
        for file in &to_repair {
            let resolved_path = folder.file(&file.file_path);
            if resolved_path.exists()
                && let Err(e) = tokio::fs::remove_file(&resolved_path).await
            {
                tracing::warn!(
                    model_id = model_id,
                    file_path = %file.file_path,
                    error = %e,
                    "Failed to delete corrupt file"
                );
                continue;
            }
            gone.push(file.file_path.clone());
        }
        if gone.is_empty() {
            return Err("No unhealthy file could be deleted, so nothing was queued".to_string());
        }

        // Queue the download and start it. The files are gone by now, so a
        // refusal says which, and what fetches them.
        let asked = quantization;
        match Arc::clone(&self.downloads)
            .queue_smart(repo_id.clone(), Some(asked.clone()), None)
            .await
        {
            Ok(download) => Ok(RepairStarted {
                id: download.to_string(),
                files: gone,
            }),
            Err(e) => Err(format!(
                "The download that fetches them again could not be queued: {e}. {}",
                missing_after_repair(&DownloadId::new(repo_id, Some(asked)), &gone)
            )),
        }
    }

    /// The rows of `files`, model `model_id`'s read in `folder`, whose
    /// files are corrupt or missing, each verified as `kinds` names it.
    async fn unhealthy<'a>(
        model_id: i64,
        folder: &ModelFolder,
        files: &'a [ModelFile],
        kinds: Vec<VerifiedFileKind>,
    ) -> Vec<&'a ModelFile> {
        let mut unhealthy = Vec::new();
        for (index, (file, kind)) in files.iter().zip(kinds).enumerate() {
            // Nobody reads this progress. The receiver is dropped here so
            // each send fails at once; kept alive, the channel fills after
            // one message and the hashing thread waits forever.
            let (tx, _) = mpsc::channel(1);
            let resolved_path = folder.file(&file.file_path);
            let place = FilePlace {
                index,
                total: files.len(),
                kind,
            };
            let health = Self::verify_shard(file, &resolved_path, model_id, place, &tx).await;
            if matches!(health, ShardHealth::Corrupt { .. } | ShardHealth::Missing) {
                unhealthy.push(file);
            }
        }
        unhealthy
    }

    /// The path of the projector a download of the variant labelled `label`
    /// from `repo_id` fetches, when one comes with it.
    async fn fetched_projector(
        &self,
        repo_id: &str,
        label: &str,
    ) -> Result<Option<String>, String> {
        let group = download_group(
            self.hf_client.as_ref(),
            Some(self.gguf_parser.as_ref()),
            repo_id,
            label,
        )
        .await
        .map_err(|e| format!("Failed to read what a download of {repo_id} fetches: {e}"))?;
        Ok(group.projector.map(|projector| projector.path))
    }
}

#[cfg(test)]
#[path = "model_verification_remote_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "model_verification_requeue_tests.rs"]
mod requeue_tests;

#[cfg(test)]
#[path = "model_verification_two_projector_tests.rs"]
mod two_projector_tests;

#[cfg(test)]
#[path = "model_verification_store_tests.rs"]
mod store_tests;

#[cfg(test)]
#[path = "model_verification_component_tests.rs"]
mod component_tests;

#[cfg(test)]
#[path = "model_verification_folder_tests.rs"]
mod folder_tests;

#[cfg(test)]
#[path = "model_verification_label_tests.rs"]
mod label_tests;
