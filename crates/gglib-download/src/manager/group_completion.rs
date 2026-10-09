//! What a finished download group is registered as.
//!
//! A group is a model's weights followed by the projector fetched with them,
//! and by an image model's companions. Its primary file, its shard list and
//! its shard count are the weights' alone; the projector and the companions
//! are handed over apart, to be linked.

use std::path::Path;

use gglib_core::domain::{ComponentRole, Model};
use gglib_core::download::ShardInfo;
use gglib_core::paths::canonical_model_path;
use gglib_core::ports::{CompletedDownload, ResolvedFile};
use gglib_core::utils::shard_filename::base_shard_filename;

use super::shard_group_tracker::{GroupComplete, GroupMetadata};
use super::worker::CompletedJob;

impl GroupMetadata {
    /// The identity every file of one group computes: the job's repository,
    /// commit and quantization, and the group's `file_entries`.
    ///
    /// The primary filename is the group's first file, a weights file, so
    /// the projector's own job answers the same metadata as the weights'.
    pub(super) fn of(completed: &CompletedJob, file_entries: Vec<ResolvedFile>) -> Self {
        let primary = file_entries
            .first()
            .map(|file| file.path.as_str())
            .or_else(|| completed.files.first().map(String::as_str));
        Self {
            repo_id: completed.repo_id.clone(),
            commit_sha: completed.commit_sha.clone(),
            quantization: completed.quantization,
            primary_filename: primary.map_or_else(|| "unknown".to_string(), base_shard_filename),
            hf_tags: vec![],
            file_entries,
        }
    }

    /// How many files the group is complete at: every file queued for it,
    /// the projector included. `shard` is the place of the file just
    /// finished, and stands in when the group's files were not kept.
    pub(super) fn expected_files(&self, shard: &ShardInfo) -> u32 {
        if self.file_entries.is_empty() {
            shard.total_shards
        } else {
            u32::try_from(self.file_entries.len()).unwrap_or(u32::MAX)
        }
    }
}

impl GroupComplete {
    /// The download as the registrar takes it: the weights as the model's
    /// files, and the projector and the companions apart.
    pub(super) fn into_completed_download(self, hf_tags: Vec<String>) -> CompletedDownload {
        let entries = &self.metadata.file_entries;
        let mut weights = Vec::new();
        let mut projectors = Vec::new();
        let mut components = Vec::new();
        for (index, path) in self.ordered_paths.iter().cloned().enumerate() {
            match entries.get(index) {
                Some(entry) if entry.role.is_projector() => projectors.push(path),
                Some(entry) => match entry.component {
                    Some(role) => components.push((role, path)),
                    None => weights.push(path),
                },
                None => weights.push(path),
            }
        }
        let primary_path = weights
            .first()
            .cloned()
            .expect("GroupComplete should have at least one weights path");

        CompletedDownload {
            primary_path,
            projector_path: projectors.into_iter().next(),
            components,
            is_sharded: weights.len() > 1,
            file_paths: (weights.len() > 1).then_some(weights),
            all_paths: self.ordered_paths,
            quantization: self.metadata.quantization,
            repo_id: self.metadata.repo_id,
            commit_sha: self.metadata.commit_sha,
            hf_tags,
            hf_file_entries: self.metadata.file_entries,
        }
    }
}

/// The roles of `download`'s companions that `model`, as registered, links
/// to the very files the download brought, in the group's order.
pub(super) fn linked_components(download: &CompletedDownload, model: &Model) -> Vec<ComponentRole> {
    download
        .components
        .iter()
        .filter(|(role, path)| {
            let path = canonical_model_path(path).unwrap_or_else(|_| path.clone());
            model
                .components
                .iter()
                .any(|link| link.role == *role && link.path == path)
        })
        .map(|(role, _)| *role)
        .collect()
}

/// How many weights shards `download` is made of.
pub(super) fn shard_count(download: &CompletedDownload) -> usize {
    download.file_paths.as_ref().map_or(1, Vec::len)
}

/// The message a finished download is announced with.
///
/// It counts the weights' shards, names the projector that was linked, and
/// says why a projector that came with the weights was not. When the reader
/// refused the weights, it names the file added without its details and
/// gives the reader's reason.
pub(super) fn completion_message(
    download: &CompletedDownload,
    metadata_refusal: Option<&str>,
    projector_refusal: Option<&str>,
) -> String {
    let what = if download.is_sharded {
        format!("{} shards", shard_count(download))
    } else {
        "model".to_string()
    };
    let downloaded = format!("Downloaded {what} to {}", download.primary_path.display());
    let projector = download.projector_path.as_deref().map(file_name);
    let message = match (projector, projector_refusal) {
        (Some(name), Some(refusal)) => {
            format!("{downloaded}. Its projector {name} was not linked: {refusal}")
        }
        (Some(name), None) => format!("{downloaded}, with its projector {name}"),
        (None, _) => downloaded,
    };
    match metadata_refusal {
        Some(refusal) => format!(
            "{message}. {} was added without its details: {refusal}",
            file_name(&download.primary_path)
        ),
        None => message,
    }
}

/// `message` with what became of an image model's companions: the roles
/// linked, `linked`, and why each one that came with it was not, `refusals`.
/// A download that brought none is announced as it was.
pub(super) fn with_companions(
    message: String,
    linked: &[ComponentRole],
    refusals: &[String],
) -> String {
    let message = if linked.is_empty() {
        message
    } else {
        let roles: Vec<&str> = linked.iter().map(|role| role.as_str()).collect();
        format!("{message}. Linked its components: {}", roles.join(", "))
    };
    refusals.iter().fold(message, |message, refusal| {
        format!("{message}. A component was not linked: {refusal}")
    })
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
#[path = "group_completion_tests.rs"]
mod tests;
