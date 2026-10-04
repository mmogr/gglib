//! What a finished download group is registered as.
//!
//! A group is a model's weights followed by the projector fetched with them.
//! Its primary file, its shard list and its shard count are the weights'
//! alone; the projector is handed over apart, to be linked.

use std::path::Path;

use gglib_core::download::ShardInfo;
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
    /// files, and the projector apart.
    pub(super) fn into_completed_download(self, hf_tags: Vec<String>) -> CompletedDownload {
        let entries = &self.metadata.file_entries;
        let is_projector = |index: usize| entries.get(index).is_some_and(|f| f.role.is_projector());
        let (projectors, weights): (Vec<_>, Vec<_>) = self
            .ordered_paths
            .iter()
            .cloned()
            .enumerate()
            .partition(|(index, _)| is_projector(*index));
        let weights: Vec<_> = weights.into_iter().map(|(_, path)| path).collect();
        let primary_path = weights
            .first()
            .cloned()
            .expect("GroupComplete should have at least one weights path");

        CompletedDownload {
            primary_path,
            projector_path: projectors.into_iter().next().map(|(_, path)| path),
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

/// How many weights shards `download` is made of.
pub(super) fn shard_count(download: &CompletedDownload) -> usize {
    download.file_paths.as_ref().map_or(1, Vec::len)
}

/// The message a finished download is announced with.
///
/// It counts the weights' shards, names the projector that was linked, and
/// says why a projector that came with the weights was not.
pub(super) fn completion_message(
    download: &CompletedDownload,
    projector_refusal: Option<&str>,
) -> String {
    let what = if download.is_sharded {
        format!("{} shards", shard_count(download))
    } else {
        "model".to_string()
    };
    let downloaded = format!("Downloaded {what} to {}", download.primary_path.display());
    let projector = download.projector_path.as_deref().map(file_name);
    match (projector, projector_refusal) {
        (Some(name), Some(refusal)) => {
            format!("{downloaded}. Its projector {name} was not linked: {refusal}")
        }
        (Some(name), None) => format!("{downloaded}, with its projector {name}"),
        (None, _) => downloaded,
    }
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
