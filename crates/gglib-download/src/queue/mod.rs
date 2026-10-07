#![doc = include_str!("README.md")]
pub(crate) mod group_items;
mod rows;
mod shard_group;
mod types;

use std::collections::VecDeque;

use gglib_core::download::{
    CompletionKey, DownloadError, DownloadId, DownloadOutcome, FINISHED_LIMIT, FinishedDownload,
    QueueSnapshot,
};
use gglib_core::ports::ResolvedFile;

pub(crate) use rows::{Reading, Running, running_row};
pub(crate) use shard_group::ShardGroupId;
pub(crate) use types::QueuedItem;

/// Saturating conversion from usize to u32 for queue positions.
/// Returns `u32::MAX` if the value exceeds `u32::MAX`.
fn usize_to_u32_saturating(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Manages the download queue state.
///
/// This is a sync type with no internal locking — the caller
/// (`DownloadManager`) is responsible for synchronization.
pub(crate) struct DownloadQueue {
    pending: VecDeque<QueuedItem>,
    /// How the most recent downloads ended, oldest first: one entry per
    /// download, and at most [`FINISHED_LIMIT`].
    finished: Vec<FinishedDownload>,
    max_size: u32,
}

impl DownloadQueue {
    /// Create a new download queue with the specified max size.
    pub(crate) const fn new(max_size: u32) -> Self {
        Self {
            pending: VecDeque::new(),
            finished: Vec::new(),
            max_size,
        }
    }

    /// Get the maximum queue size.
    #[cfg(test)]
    pub(crate) const fn max_size(&self) -> u32 {
        self.max_size
    }

    /// Set the maximum queue size.
    pub(crate) const fn set_max_size(&mut self, size: u32) {
        self.max_size = size;
    }

    /// Get the number of pending items.
    #[cfg(test)]
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// How the most recent downloads ended, oldest first.
    #[cfg(test)]
    pub(crate) fn finished(&self) -> &[FinishedDownload] {
        &self.finished
    }

    /// Check if a download ID is waiting in `pending`.
    ///
    /// Pending only — an item that has started downloading has left this queue
    /// for the manager's `active` map and will not be found here. Callers
    /// guarding against duplicate work need to check both.
    pub(crate) fn is_queued(&self, id: &DownloadId) -> bool {
        self.pending.iter().any(|item| &item.id == id)
    }

    /// Queue a single (non-sharded) download.
    ///
    /// Returns the 1-based queue position on success.
    ///
    /// Note: Production code uses `queue_sharded` for all downloads.
    /// This method is primarily for testing single-item queue behavior.
    #[cfg(test)]
    pub(crate) fn queue(
        &mut self,
        id: DownloadId,
        completion_key: CompletionKey,
        has_active: bool,
    ) -> Result<u32, DownloadError> {
        self.check_not_queued(&id)?;
        self.check_capacity(None)?;
        self.forget_outcome(&id);

        let item = QueuedItem::new(id, completion_key);
        self.pending.push_back(item);

        // Position: if something is active, pending starts at 2
        let position = if has_active {
            usize_to_u32_saturating(self.pending.len()).saturating_add(1)
        } else {
            usize_to_u32_saturating(self.pending.len())
        };

        Ok(position)
    }

    /// Queue a download group: every file of one model, with a shared
    /// `group_id`. The weights come first; a projector is the last file.
    ///
    /// The group is one download however many files it has. Returns its
    /// 1-based position among the downloads, behind `running` and every
    /// download already waiting.
    pub(crate) fn queue_sharded(
        &mut self,
        id: &DownloadId,
        completion_key: &CompletionKey,
        shard_files: &[ResolvedFile],
        running: Option<&DownloadId>,
    ) -> Result<u32, DownloadError> {
        if shard_files.is_empty() {
            return Err(DownloadError::not_in_queue(id.to_string()));
        }

        self.check_not_queued(id)?;
        self.check_capacity(running)?;
        self.forget_outcome(id);

        let position = rows::first_waiting_position(running.is_some())
            .saturating_add(usize_to_u32_saturating(self.waiting(running).len()));

        let items = group_items::group_items(id, completion_key, shard_files);
        self.pending.extend(items);

        Ok(position)
    }

    /// Pop the next item from the front of the queue.
    pub(crate) fn dequeue(&mut self) -> Option<QueuedItem> {
        self.pending.pop_front()
    }

    /// Take every pending file of the download `id` off the queue, and
    /// answer them in order.
    pub(crate) fn take_pending(&mut self, id: &DownloadId) -> Vec<QueuedItem> {
        let (taken, kept): (VecDeque<_>, VecDeque<_>) =
            self.pending.drain(..).partition(|item| &item.id == id);
        self.pending = kept;
        taken.into()
    }

    /// The downloads with a file pending, each once, in the order they run.
    pub(crate) fn pending_ids(&self) -> Vec<DownloadId> {
        let first_files = self.waiting(None);
        first_files
            .into_iter()
            .map(|item| item.id.clone())
            .collect()
    }

    /// Move a waiting download, every file of it, to a new position.
    ///
    /// Positions count downloads, as the snapshot numbers them: the first
    /// waiting download is at 2 behind `running`, and at 1 when nothing is
    /// running. The pending files of `running` stay at the head of the queue:
    /// nothing is placed before them, and they are not moved.
    ///
    /// Returns the 1-based position the download now holds.
    pub(crate) fn reorder(
        &mut self,
        id: &DownloadId,
        new_position: u32,
        running: Option<&DownloadId>,
    ) -> Result<u32, DownloadError> {
        if !self.is_queued(id) {
            return Err(DownloadError::not_in_queue(id.to_string()));
        }
        if running == Some(id) {
            return Ok(1);
        }

        let (moved, kept): (VecDeque<_>, VecDeque<_>) =
            self.pending.drain(..).partition(|item| &item.id == id);
        self.pending = kept;

        let first = rows::first_waiting_position(running.is_some());
        let slot = (new_position.saturating_sub(first) as usize).min(self.waiting(running).len());
        let index = self.waiting_index(running, slot);
        for (offset, item) in moved.into_iter().enumerate() {
            self.pending.insert(index + offset, item);
        }

        Ok(first.saturating_add(usize_to_u32_saturating(slot)))
    }

    /// The queue as it is served: the running download, the waiting ones
    /// behind it, and how the latest ended.
    ///
    /// `running` names the running download, being fetched or between two of
    /// its files, and `reading` is its meter's. Each waiting download is one
    /// row however many files it has, and the pending files of the running
    /// download are no row at all.
    pub(crate) fn snapshot(
        &self,
        revision: u64,
        running: Option<&Running>,
        reading: Option<&Reading>,
    ) -> QueueSnapshot {
        let (active, waiting) = self.download_rows(running, reading);
        QueueSnapshot {
            revision,
            active,
            full: waiting.len() >= self.max_size as usize,
            waiting,
            finished: self.finished.clone(),
            max_size: self.max_size,
        }
    }

    /// Record how the download `id` ended, in place of any earlier ending of
    /// it, and answer the entry. The oldest make way past [`FINISHED_LIMIT`].
    pub(crate) fn record_outcome(
        &mut self,
        id: &DownloadId,
        outcome: DownloadOutcome,
    ) -> FinishedDownload {
        self.forget_outcome(id);
        let ended = FinishedDownload::new(id, outcome);
        self.finished.push(ended.clone());
        let excess = self.finished.len().saturating_sub(FINISHED_LIMIT);
        self.finished.drain(..excess);
        ended
    }

    // --- Private helpers ---

    fn check_not_queued(&self, id: &DownloadId) -> Result<(), DownloadError> {
        if self.is_queued(id) {
            Err(DownloadError::already_queued(id.to_string()))
        } else {
            Ok(())
        }
    }

    /// Room for one more waiting download. The files of `running` take no
    /// place, and a download is one place however many files it has.
    fn check_capacity(&self, running: Option<&DownloadId>) -> Result<(), DownloadError> {
        if self.waiting(running).len() >= self.max_size as usize {
            Err(DownloadError::queue_full(self.max_size))
        } else {
            Ok(())
        }
    }

    /// Drop what is recorded of an earlier run of `id`, and answer whether
    /// there was anything. A download queued again starts clean: its old
    /// ending is not this run's.
    pub(crate) fn forget_outcome(&mut self, id: &DownloadId) -> bool {
        let before = self.finished.len();
        let id = id.to_string();
        self.finished.retain(|entry| entry.id != id);
        self.finished.len() < before
    }
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(error: &str) -> DownloadOutcome {
        DownloadOutcome::Failed {
            error: error.to_string(),
        }
    }

    /// Name the download of `item`, just dequeued, as the running one.
    fn running(item: QueuedItem) -> Running {
        Running {
            id: item.id,
            phase: gglib_core::download::DownloadPhase::Downloading,
            file: item.shard_info,
        }
    }

    fn test_id(model: &str, quant: Option<&str>) -> DownloadId {
        DownloadId::new(model, quant)
    }

    fn test_completion_key(id: &DownloadId) -> CompletionKey {
        CompletionKey::HfFile {
            repo_id: id.model_id().to_string(),
            revision: "unspecified".to_string(),
            filename_canon: "test.gguf".to_string(),
            quantization: id.quantization().map(String::from),
        }
    }

    #[test]
    fn test_queue_single_download() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/a", Some("Q4_K_M"));
        let key = test_completion_key(&id);
        let pos = queue.queue(id.clone(), key, false).unwrap();

        assert_eq!(pos, 1); // 1-based, no active
        assert!(queue.is_queued(&id));
        assert_eq!(queue.pending_len(), 1);
    }

    #[test]
    fn test_queue_with_active() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/a", None);
        let pos = queue
            .queue(id.clone(), test_completion_key(&id), true)
            .unwrap(); // has_active = true

        assert_eq!(pos, 2); // Position 1 is active, so this is 2
    }

    #[test]
    fn test_queue_multiple() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        let id_c = test_id("c", None);
        let pos1 = queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        let pos2 = queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();
        let pos3 = queue
            .queue(id_c.clone(), test_completion_key(&id_c), false)
            .unwrap();

        assert_eq!(pos1, 1);
        assert_eq!(pos2, 2);
        assert_eq!(pos3, 3);
    }

    #[test]
    fn test_queue_rejects_duplicate() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/a", None);
        queue
            .queue(id.clone(), test_completion_key(&id), false)
            .unwrap();

        let result = queue.queue(id.clone(), test_completion_key(&id), false);
        assert!(matches!(result, Err(DownloadError::AlreadyQueued { .. })));
    }

    #[test]
    fn test_queue_respects_capacity() {
        let mut queue = DownloadQueue::new(2);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        let id_c = test_id("c", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        let result = queue.queue(id_c.clone(), test_completion_key(&id_c), false);
        assert!(matches!(
            result,
            Err(DownloadError::QueueFull { max_size: 2 })
        ));
    }

    #[test]
    fn test_dequeue_fifo() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        let first = queue.dequeue().unwrap();
        assert_eq!(first.id.model_id(), "a");

        let second = queue.dequeue().unwrap();
        assert_eq!(second.id.model_id(), "b");

        assert!(queue.dequeue().is_none());
    }

    #[test]
    fn test_snapshot_positions() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        // Simulate "a" is now active
        let current = queue.dequeue().unwrap();
        let current = running(current);
        let snapshot = queue.snapshot(0, Some(&current), None);

        assert_eq!(snapshot.rows().count(), 2); // 1 active + 1 pending
        assert_eq!(snapshot.rows().next().unwrap().position, 1);
        assert_eq!(snapshot.rows().nth(1).unwrap().position, 2);
        assert_eq!(u32::from(snapshot.active.is_some()), 1);
        assert_eq!(snapshot.waiting.len(), 1);
    }

    #[test]
    fn test_sharded_download() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/x", Some("Q4_K_M"));
        let shards = vec![
            ResolvedFile::with_size("shard-001.gguf", 1000),
            ResolvedFile::with_size("shard-002.gguf", 2000),
        ];

        let key = test_completion_key(&id);
        let pos = queue.queue_sharded(&id, &key, &shards, None).unwrap();
        assert_eq!(pos, 1);
        assert_eq!(queue.pending_len(), 2);

        // Both files share one group, and the two of them are one row
        let group_id = queue.pending[0].group_id.clone().unwrap();
        assert_eq!(queue.pending[1].group_id.as_ref(), Some(&group_id));
        let snapshot = queue.snapshot(0, None, None);
        assert_eq!(snapshot.rows().count(), 1);
        assert_eq!(snapshot.waiting.len(), 1);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tests for what the port's queue methods do here: remove, reorder,
    // the finished list, max_size
    // ─────────────────────────────────────────────────────────────────────────

    /// Taking a download off takes every pending file of it, in order, and
    /// no other download's.
    #[test]
    fn taking_a_download_takes_every_pending_file_of_it() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        let shards = [ResolvedFile::new("s1.gguf"), ResolvedFile::new("s2.gguf")];
        let key = test_completion_key(&id_a);
        queue.queue_sharded(&id_a, &key, &shards, None).unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();
        assert_eq!(queue.pending_ids(), [id_a.clone(), id_b.clone()]);

        let taken = queue.take_pending(&id_a);

        let names: Vec<_> = taken
            .iter()
            .map(|item| item.shard_info.as_ref().unwrap().filename.as_str())
            .collect();
        assert_eq!(names, ["s1.gguf", "s2.gguf"]);
        assert!(!queue.is_queued(&id_a));
        assert_eq!(queue.pending_ids(), [id_b]);
        assert!(queue.take_pending(&id_a).is_empty(), "nothing left of it");
    }

    #[test]
    fn forgetting_a_finished_download_drops_its_outcome_alone() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("a", None);
        queue.record_outcome(&id, failed("error"));
        queue.record_outcome(&test_id("b", None), failed("error"));

        assert!(queue.forget_outcome(&id));

        let left: Vec<_> = queue.finished().iter().map(|f| f.id.as_str()).collect();
        assert_eq!(left, ["b"]);
        assert!(!queue.forget_outcome(&id), "nothing left to forget");
    }

    #[test]
    fn test_reorder_to_front() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        let id_c = test_id("c", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();
        queue
            .queue(id_c.clone(), test_completion_key(&id_c), false)
            .unwrap();

        // Move "c" to position 1
        let new_pos = queue.reorder(&test_id("c", None), 1, None).unwrap();

        assert_eq!(new_pos, 1);
        let ids: Vec<_> = queue.pending.iter().map(|i| i.id.model_id()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    #[test]
    fn test_reorder_to_middle() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        let id_c = test_id("c", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();
        queue
            .queue(id_c.clone(), test_completion_key(&id_c), false)
            .unwrap();

        // Move "c" to position 2
        let new_pos = queue.reorder(&test_id("c", None), 2, None).unwrap();

        assert_eq!(new_pos, 2);
        let ids: Vec<_> = queue.pending.iter().map(|i| i.id.model_id()).collect();
        assert_eq!(ids, vec!["a", "c", "b"]);
    }

    #[test]
    fn test_reorder_with_active() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        let id_c = test_id("c", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();
        queue
            .queue(id_c.clone(), test_completion_key(&id_c), false)
            .unwrap();

        // Another download is running, so position 1 is taken
        // Move "c" to position 2 (first waiting place)
        let running = test_id("z", None);
        let new_pos = queue
            .reorder(&test_id("c", None), 2, Some(&running))
            .unwrap();

        assert_eq!(new_pos, 2);
        let ids: Vec<_> = queue.pending.iter().map(|i| i.id.model_id()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    #[test]
    fn test_reorder_not_found() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();

        let result = queue.reorder(&test_id("nonexistent", None), 1, None);
        assert!(matches!(result, Err(DownloadError::NotInQueue { .. })));
    }

    #[test]
    fn test_reorder_shard_group() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();

        let id_sharded = test_id("sharded", Some("Q4"));
        let shards = vec![ResolvedFile::new("s1.gguf"), ResolvedFile::new("s2.gguf")];
        queue
            .queue_sharded(
                &id_sharded.clone(),
                &test_completion_key(&id_sharded),
                &shards,
                None,
            )
            .unwrap();

        let id_b = test_id("b", None);
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        // Move shard group to front - both shards should move together
        let new_pos = queue.reorder(&id_sharded, 1, None).unwrap();

        assert_eq!(new_pos, 1);
        let ids: Vec<_> = queue.pending.iter().map(|i| i.id.model_id()).collect();
        // Both shards at front, then a, then b
        assert_eq!(ids, vec!["sharded", "sharded", "a", "b"]);
    }

    /// An outcome carries the download's id and title, and the snapshot
    /// serves the list oldest first.
    #[test]
    fn an_outcome_is_served_with_the_downloads_title() {
        let mut queue = DownloadQueue::new(3);
        let id = test_id("owner/model-a", Some("Q8_0"));
        queue.record_outcome(&id, failed("connection timeout"));
        queue.record_outcome(&test_id("b", None), DownloadOutcome::Cancelled);

        let snapshot = queue.snapshot(0, None, None);

        assert!(snapshot.is_idle());
        assert_eq!(snapshot.finished.len(), 2);
        assert_eq!(snapshot.finished[0].id, "owner/model-a:Q8_0");
        assert_eq!(snapshot.finished[0].title, "owner/model-a:Q8_0");
        assert_eq!(snapshot.finished[0].outcome, failed("connection timeout"));
        assert_eq!(snapshot.finished[1].outcome, DownloadOutcome::Cancelled);
    }

    /// A download that ends again has one entry, its latest, and it is the
    /// newest in the list.
    #[test]
    fn the_newest_outcome_of_a_download_replaces_its_last() {
        let mut queue = DownloadQueue::new(3);
        let (a, b) = (test_id("a", None), test_id("b", None));
        queue.record_outcome(&a, failed("first try"));
        queue.record_outcome(&b, DownloadOutcome::Cancelled);

        queue.record_outcome(&a, DownloadOutcome::Completed { message: None });

        let ended: Vec<_> = queue
            .finished()
            .iter()
            .map(|f| (f.id.as_str(), &f.outcome))
            .collect();
        assert_eq!(
            ended,
            [
                ("b", &DownloadOutcome::Cancelled),
                ("a", &DownloadOutcome::Completed { message: None }),
            ]
        );
    }

    /// The list keeps the latest `FINISHED_LIMIT`, and the oldest make way.
    #[test]
    fn the_finished_list_is_bounded() {
        let mut queue = DownloadQueue::new(3);
        for n in 0..FINISHED_LIMIT + 4 {
            queue.record_outcome(&test_id(&format!("m{n}"), None), DownloadOutcome::Cancelled);
        }

        let finished = queue.finished();

        assert_eq!(finished.len(), FINISHED_LIMIT);
        assert_eq!(finished[0].id, "m4");
        assert_eq!(
            finished[FINISHED_LIMIT - 1].id,
            format!("m{}", FINISHED_LIMIT + 3)
        );
    }

    /// Queued again, a download starts clean: the monitor watching it would
    /// otherwise read its last run's failure as this one's.
    #[test]
    fn queueing_a_download_again_forgets_its_old_outcome() {
        let mut queue = DownloadQueue::new(3);
        let id = test_id("a", Some("Q8_0"));
        queue.record_outcome(&id, failed("first try"));
        queue.record_outcome(&test_id("b", None), failed("another"));

        queue
            .queue_sharded(
                &id,
                &test_completion_key(&id),
                &[ResolvedFile::new("a.gguf")],
                None,
            )
            .unwrap();

        let left: Vec<_> = queue.finished().iter().map(|f| f.id.as_str()).collect();
        assert_eq!(left, ["b"]);
    }

    #[test]
    fn test_set_and_get_max_size() {
        let mut queue = DownloadQueue::new(5);
        assert_eq!(queue.max_size(), 5);

        queue.set_max_size(20);
        assert_eq!(queue.max_size(), 20);
    }

    #[test]
    fn test_max_size_change_preserves_items() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        // Reduce max size below current count
        queue.set_max_size(1);

        // Existing items preserved
        assert_eq!(queue.pending_len(), 2);
        assert_eq!(queue.max_size(), 1);

        // But can't add more
        let id_c = test_id("c", None);
        let result = queue.queue(id_c.clone(), test_completion_key(&id_c), false);
        assert!(matches!(result, Err(DownloadError::QueueFull { .. })));
    }

    /// Test that enqueue → dequeue follows strict FIFO ordering with capacity 3.
    #[test]
    fn test_enqueue_dequeue_fifo_ordering() {
        let mut queue = DownloadQueue::new(3);

        // Enqueue 3 items with distinct IDs
        let id_a = test_id("model-a", None);
        let id_b = test_id("model-b", None);
        let id_c = test_id("model-c", None);

        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();
        queue
            .queue(id_c.clone(), test_completion_key(&id_c), false)
            .unwrap();

        // Dequeue in FIFO order
        let first = queue.dequeue().unwrap();
        assert_eq!(first.id.model_id(), "model-a");

        let second = queue.dequeue().unwrap();
        assert_eq!(second.id.model_id(), "model-b");

        let third = queue.dequeue().unwrap();
        assert_eq!(third.id.model_id(), "model-c");

        // Queue is now empty - dequeue returns None
        assert!(queue.dequeue().is_none());
    }

    /// Test that positions shift down after dequeue.
    #[test]
    fn test_position_tracking_on_enqueue_dequeue() {
        let mut queue = DownloadQueue::new(3);

        // Enqueue 3 items: "a", "b", "c"
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        let id_c = test_id("c", None);

        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();
        queue
            .queue(id_c.clone(), test_completion_key(&id_c), false)
            .unwrap();

        // Snapshot: positions should be 1, 2, 3
        let snapshot = queue.snapshot(0, None, None);
        assert_eq!(snapshot.rows().count(), 3);
        assert_eq!(snapshot.rows().next().unwrap().position, 1);
        assert_eq!(snapshot.rows().next().unwrap().id, "a");
        assert_eq!(snapshot.rows().nth(1).unwrap().position, 2);
        assert_eq!(snapshot.rows().nth(1).unwrap().id, "b");
        assert_eq!(snapshot.rows().nth(2).unwrap().position, 3);
        assert_eq!(snapshot.rows().nth(2).unwrap().id, "c");

        // Dequeue "a"
        let dequeued = queue.dequeue().unwrap();
        assert_eq!(dequeued.id.model_id(), "a");

        // Snapshot again: positions should have shifted down
        let snapshot = queue.snapshot(0, None, None);
        assert_eq!(snapshot.rows().count(), 2);
        assert_eq!(snapshot.rows().next().unwrap().position, 1);
        assert_eq!(snapshot.rows().next().unwrap().id, "b");
        assert_eq!(snapshot.rows().nth(1).unwrap().position, 2);
        assert_eq!(snapshot.rows().nth(1).unwrap().id, "c");
    }

    /// Test that `snapshot()` produces correct DTOs with active/pending counts.
    #[test]
    fn test_snapshot_produces_correct_dtos() {
        let mut queue = DownloadQueue::new(5);

        // Enqueue 2 items: "model-x" and "model-y"
        let id_x = test_id("model-x", None);
        let id_y = test_id("model-y", None);

        queue
            .queue(id_x.clone(), test_completion_key(&id_x), false)
            .unwrap();
        queue
            .queue(id_y.clone(), test_completion_key(&id_y), false)
            .unwrap();

        // Snapshot with nothing active: 0 active, 2 pending
        let snapshot = queue.snapshot(0, None, None);
        assert_eq!(snapshot.rows().count(), 2);
        assert_eq!(u32::from(snapshot.active.is_some()), 0);
        assert_eq!(snapshot.waiting.len(), 2);
        assert_eq!(snapshot.rows().next().unwrap().position, 1);
        assert_eq!(snapshot.rows().next().unwrap().id, "model-x");
        assert_eq!(snapshot.rows().nth(1).unwrap().position, 2);
        assert_eq!(snapshot.rows().nth(1).unwrap().id, "model-y");

        // Dequeue "model-x" and mark it as downloading
        let current = queue.dequeue().unwrap();
        assert_eq!(current.id.model_id(), "model-x");
        let current = running(current);

        // Snapshot with active item: 1 active, 1 pending
        let snapshot = queue.snapshot(0, Some(&current), None);
        assert_eq!(u32::from(snapshot.active.is_some()), 1);
        assert_eq!(snapshot.waiting.len(), 1);
    }

    /// Test that dequeue on an empty queue returns None gracefully.
    #[test]
    fn test_dequeue_from_empty_queue_returns_none() {
        let mut queue = DownloadQueue::new(3);

        // Dequeue immediately — should return None, not panic or deadlock
        assert!(queue.dequeue().is_none());
    }

    /// Test that concurrent `snapshot()` reads complete without deadlock.
    /// Because `DownloadQueue` is wrapped in a `tokio::sync::RwLock`, multiple
    /// readers can hold shared references simultaneously — all snapshots
    /// should complete quickly and return consistent queued-item counts.
    #[tokio::test]
    async fn test_concurrent_snapshot_reads() {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let mut queue = DownloadQueue::new(10);

        // Enqueue 2 items so the queue is non-empty
        let id_a = test_id("concurrent-a", None);
        let id_b = test_id("concurrent-b", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        let queue = Arc::new(RwLock::new(queue));

        // Launch 3 concurrent tasks, each reading a snapshot
        let (s1, s2, s3) = tokio::join!(
            async { queue.read().await.snapshot(0, None, None) },
            async { queue.read().await.snapshot(0, None, None) },
            async { queue.read().await.snapshot(0, None, None) },
        );

        // All three snapshots should succeed and agree on queued items count
        assert_eq!(s1.rows().count(), 2);
        assert_eq!(s2.rows().count(), 2);
        assert_eq!(s3.rows().count(), 2);

        // Each snapshot should have the same pending count
        assert_eq!(s1.waiting.len(), 2);
        assert_eq!(s2.waiting.len(), 2);
        assert_eq!(s3.waiting.len(), 2);
    }

    /// Test that enqueue can proceed while a snapshot reader is active.
    /// The writer (enqueue) waits for the reader to release the lock, then proceeds.
    #[tokio::test]
    async fn test_enqueue_while_snapshot_reading() {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let mut queue = DownloadQueue::new(10);

        // Enqueue 1 initial item
        let id_a = test_id("reader-item", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();

        let queue = Arc::new(RwLock::new(queue));

        // Spawn a "reader" task that holds the read lock for 50ms
        let reader_queue = Arc::clone(&queue);
        let reader = tokio::spawn(async move {
            let guard = reader_queue.read().await;
            let snapshot = guard.snapshot(0, None, None);
            drop(guard); // Release read lock before sleeping
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            snapshot
        });

        // Spawn an "enqueuer" task that adds a new item while reader is active
        let enqueuer_queue = Arc::clone(&queue);
        let id_b = test_id("writer-item", None);
        let enqueuer = tokio::spawn(async move {
            let mut guard = enqueuer_queue.write().await;
            let key = test_completion_key(&id_b);
            guard.queue(id_b.clone(), key, false).unwrap();
            id_b
        });

        // Wait for both tasks to complete
        let reader_snapshot = reader.await.unwrap();
        let enqueued_id = enqueuer.await.unwrap();

        // Reader saw 1 item (before enqueue)
        assert_eq!(reader_snapshot.rows().count(), 1);

        // Enqueuer completed successfully
        assert_eq!(enqueued_id.model_id(), "writer-item");

        // Final snapshot should show 2 items
        let final_snapshot = queue.read().await.snapshot(0, None, None);
        assert_eq!(final_snapshot.rows().count(), 2);
        assert_eq!(final_snapshot.waiting.len(), 2);
    }

    /// Test that concurrent enqueue writes are serialized safely by `RwLock`.
    /// Multiple tasks enqueue simultaneously; none should be lost.
    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn test_concurrent_enqueue_writes() {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let queue = Arc::new(RwLock::new(DownloadQueue::new(10)));

        // Spawn 8 concurrent tasks, each enqueuing a unique item
        let h0 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-0", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };
        let h1 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-1", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };
        let h2 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-2", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };
        let h3 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-3", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };
        let h4 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-4", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };
        let h5 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-5", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };
        let h6 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-6", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };
        let h7 = {
            let q = Arc::clone(&queue);
            tokio::spawn(async move {
                let id = test_id("concurrent-7", None);
                let key = test_completion_key(&id);
                let mut guard = q.write().await;
                guard.queue(id.clone(), key, true).unwrap();
                id
            })
        };

        // Wait for all tasks to complete using tokio::join!
        let (r0, r1, r2, r3, r4, r5, r6, r7) = tokio::join!(h0, h1, h2, h3, h4, h5, h6, h7);
        // All tasks should have completed successfully
        r0.unwrap();
        r1.unwrap();
        r2.unwrap();
        r3.unwrap();
        r4.unwrap();
        r5.unwrap();
        r6.unwrap();
        r7.unwrap();

        // Dequeue all items and verify none were lost
        let mut dequeued = Vec::new();
        loop {
            let item = queue.write().await.dequeue();
            match item {
                Some(q_item) => dequeued.push(q_item.id),
                None => break,
            }
        }

        // Exactly 8 items should have been dequeued
        assert_eq!(dequeued.len(), 8);
    }
}
