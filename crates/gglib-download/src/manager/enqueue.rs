//! Putting a resolved download group on the queue.

use gglib_core::download::{CompletionKey, DownloadError, DownloadId};
use gglib_core::ports::Resolution;
use gglib_core::utils::shard_filename::base_shard_filename;

use super::DownloadManagerImpl;

impl DownloadManagerImpl {
    /// Queue every file of `resolution` as one group under `id`, and keep
    /// the group's files for registration.
    ///
    /// Returns the group's 1-based position among the downloads, or `None`
    /// when `id` is already in flight and nothing was queued.
    ///
    /// Every way of queueing comes through here, so an id is on the queue or
    /// running at most once, and a row or a progress bar can be keyed by it.
    /// A download queued here starts with no meter, whatever an earlier run
    /// of the same id left.
    pub(super) async fn enqueue_group(
        &self,
        id: &DownloadId,
        revision: Option<&str>,
        resolution: &Resolution,
    ) -> Result<Option<u32>, DownloadError> {
        let completion_key = completion_key(id, revision, resolution)?;

        // Minimal lock scope: find what is running and mutate the queue
        let position = {
            let mut queue = self.queue.write().await;
            let running = self.running_id(&queue).await;

            // A repeat request for a download already in flight attaches to
            // it instead of enqueueing a second copy. `is_queued` scans only
            // `pending`, so a file that has moved to `active` is asked after
            // too: a retried `gglib model download`, or a repair of a model
            // already being fetched, would otherwise queue the group a second
            // time under the running id.
            //
            // Deliberately narrower than "does the queue know this id": a
            // check that also matched a *failed* download would make failures
            // permanently un-retryable. `queue_sharded` forgets the id's old
            // outcome instead, so a download that ended can be queued again.
            //
            // The check and the enqueue share one queue guard, taken before
            // `active`, so two requests for one id cannot both pass it.
            if queue.is_queued(id) || running.as_ref() == Some(id) {
                tracing::info!(
                    id = %id,
                    "Download already in flight - attaching rather than queueing a duplicate"
                );
                return Ok(None);
            }

            // A download queued again starts from nothing. The id is neither
            // waiting nor running here, so a meter under it is left over: a
            // run that ended with a file still to come, or was taken off the
            // queue between two files. It goes under the queue guard, before
            // `next_job` can find it. Lock order: queue → meters.
            self.meters().remove(id);

            queue.queue_sharded(id, &completion_key, &resolution.files, running.as_ref())?
        };

        // The files with their OIDs and roles, for registration and for
        // telling when the whole group is on disk
        self.file_entries_map
            .lock()
            .await
            .insert(id.to_string(), resolution.files.clone());

        Ok(Some(position))
    }
}

/// The identity a group completes under: its first file, which is a weights
/// file, with any shard numbering removed.
fn completion_key(
    id: &DownloadId,
    revision: Option<&str>,
    resolution: &Resolution,
) -> Result<CompletionKey, DownloadError> {
    let first_path = resolution
        .files
        .first()
        .ok_or_else(|| DownloadError::resolution_failed("no files resolved".to_string()))?
        .path
        .as_str();
    let filename = std::path::Path::new(first_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(first_path);
    Ok(CompletionKey::HfFile {
        repo_id: id.model_id().to_string(),
        revision: revision.unwrap_or("unspecified").to_string(),
        filename_canon: base_shard_filename(filename),
        quantization: Some(resolution.quantization.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::download::Quantization;
    use gglib_core::ports::ResolvedFile;

    fn resolution(files: Vec<ResolvedFile>) -> Resolution {
        Resolution {
            quantization: Quantization::Q8_0,
            files,
            is_sharded: false,
        }
    }

    /// The key names the weights even though the projector's name sorts
    /// first.
    #[test]
    fn the_completion_key_names_the_weights() {
        let id = DownloadId::new("owner/zeta-GGUF", Some("Q8_0"));
        let group = resolution(vec![
            ResolvedFile::with_size("zeta.Q8_0.gguf", 1_000),
            ResolvedFile::projector("mmproj-F16.gguf", 300, None),
        ]);

        let key = completion_key(&id, None, &group).unwrap();

        assert_eq!(
            key,
            CompletionKey::HfFile {
                repo_id: "owner/zeta-GGUF".to_string(),
                revision: "unspecified".to_string(),
                filename_canon: "zeta.Q8_0.gguf".to_string(),
                quantization: Some("Q8_0".to_string()),
            }
        );
    }

    #[test]
    fn the_completion_key_carries_a_given_revision_and_the_file_name_alone() {
        let id = DownloadId::new("owner/zeta-GGUF", Some("Q8_0"));
        let group = resolution(vec![ResolvedFile::new("Q8_0/zeta.Q8_0.gguf")]);

        let key = completion_key(&id, Some("v2"), &group).unwrap();

        let CompletionKey::HfFile {
            revision,
            filename_canon,
            ..
        } = key
        else {
            panic!("a Hub file key, not {key:?}");
        };
        assert_eq!(revision, "v2");
        assert_eq!(filename_canon, "zeta.Q8_0.gguf");
    }

    #[test]
    fn a_resolution_without_files_has_no_key() {
        let id = DownloadId::new("owner/zeta-GGUF", Some("Q8_0"));

        assert!(completion_key(&id, None, &resolution(vec![])).is_err());
    }
}
