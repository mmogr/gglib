#![doc = include_str!("README.md")]
mod group_items;
mod rows;
mod shard_group;
mod types;

use std::collections::VecDeque;

use gglib_core::download::{
    CompletionKey, DownloadError, DownloadId, DownloadStatus, QueueSnapshot, QueuedDownload,
};
use gglib_core::ports::ResolvedFile;

pub(crate) use shard_group::ShardGroupId;
pub(crate) use types::{FailedItem, QueuedItem};

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
    failed: Vec<FailedItem>,
    max_size: u32,
}

impl DownloadQueue {
    /// Create a new download queue with the specified max size.
    pub(crate) const fn new(max_size: u32) -> Self {
        Self {
            pending: VecDeque::new(),
            failed: Vec::new(),
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

    /// Get the number of failed items.
    #[cfg(test)]
    pub(crate) const fn failed_len(&self) -> usize {
        self.failed.len()
    }

    /// Check if a download ID is waiting in `pending`.
    ///
    /// Pending only — an item that has started downloading has left this queue
    /// for the manager's `active` map and will not be found here. Callers
    /// guarding against duplicate work need to check both.
    pub(crate) fn is_queued(&self, id: &DownloadId) -> bool {
        self.pending.iter().any(|item| &item.id == id)
    }

    /// Check if a download ID is in the failed list.
    #[cfg(test)]
    pub(crate) fn is_failed(&self, id: &DownloadId) -> bool {
        self.failed.iter().any(|item| &item.item.id == id)
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
        self.remove_from_failed(&id);

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
        self.remove_from_failed(id);

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

    /// Clear all items from the queue (pending and failed).
    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.failed.clear();
    }

    /// Remove an item from the pending queue or failed list.
    pub(crate) fn remove(&mut self, id: &DownloadId) -> Result<(), DownloadError> {
        let initial_pending = self.pending.len();
        self.pending.retain(|item| &item.id != id);

        if self.pending.len() < initial_pending {
            return Ok(());
        }

        let initial_failed = self.failed.len();
        self.failed.retain(|item| &item.item.id != id);

        if self.failed.len() < initial_failed {
            Ok(())
        } else {
            Err(DownloadError::not_in_queue(id.to_string()))
        }
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

    /// Get a snapshot of the current queue state for API responses.
    ///
    /// `running` is the row of the running download, if any: the one being
    /// fetched, or between two of its files. Each waiting download is one
    /// row behind it, however many files it has, and the pending files of
    /// the running download are no row at all.
    pub(crate) fn snapshot(&self, running: Option<QueuedDownload>) -> QueueSnapshot {
        let first = rows::first_waiting_position(running.is_some());

        let waiting: Vec<_> = self
            .waiting_but(|item| {
                running
                    .as_ref()
                    .is_some_and(|row| row.id == item.canonical_id())
            })
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                item.to_dto(
                    first.saturating_add(usize_to_u32_saturating(idx)),
                    DownloadStatus::Queued,
                )
            })
            .collect();

        let failed: Vec<_> = self.failed.iter().map(types::FailedItem::to_dto).collect();

        let active_count = u32::from(running.is_some());
        let pending_count = usize_to_u32_saturating(waiting.len());

        let mut items = Vec::with_capacity(1 + waiting.len());
        items.extend(running);
        items.extend(waiting);

        QueueSnapshot {
            items,
            max_size: self.max_size,
            active_count,
            pending_count,
            recent_failures: failed,
        }
    }

    /// Mark a download as failed and add to the failed list.
    pub(crate) fn mark_failed(&mut self, item: QueuedItem, error: impl Into<String>) {
        self.failed.push(FailedItem::new(item, error));
    }

    /// Clear all failed downloads.
    pub(crate) fn clear_failed(&mut self) {
        self.failed.clear();
    }

    // --- Shard group helpers ---

    /// Remove all pending items belonging to a shard group.
    pub(crate) fn remove_group(&mut self, group_id: &ShardGroupId) -> usize {
        let initial = self.pending.len();
        self.pending
            .retain(|item| item.group_id.as_ref() != Some(group_id));
        initial - self.pending.len()
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

    fn remove_from_failed(&mut self, id: &DownloadId) {
        self.failed.retain(|item| &item.item.id != id);
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
        let current_dto = current.to_dto(1, DownloadStatus::Downloading);
        let snapshot = queue.snapshot(Some(current_dto));

        assert_eq!(snapshot.items.len(), 2); // 1 active + 1 pending
        assert_eq!(snapshot.items[0].position, 1);
        assert_eq!(snapshot.items[1].position, 2);
        assert_eq!(snapshot.active_count, 1);
        assert_eq!(snapshot.pending_count, 1);
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
        let snapshot = queue.snapshot(None);
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].group_id, Some(group_id.to_string()));
        assert_eq!(snapshot.pending_count, 1);
    }

    #[test]
    fn test_remove_group() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/x", Some("Q4_K_M"));
        let shards = vec![ResolvedFile::new("s1.gguf"), ResolvedFile::new("s2.gguf")];
        let key = test_completion_key(&id);
        queue.queue_sharded(&id, &key, &shards, None).unwrap();

        let group_id = queue.pending.front().unwrap().group_id.clone().unwrap();
        let removed = queue.remove_group(&group_id);

        assert_eq!(removed, 2);
        assert!(queue.pending.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tests for new port methods: remove, reorder, retry, clear_failed, max_size
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_remove_pending_item() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        queue.remove(&id_a).unwrap();

        assert!(!queue.is_queued(&id_a));
        assert!(queue.is_queued(&id_b));
        assert_eq!(queue.pending_len(), 1);
    }

    #[test]
    fn test_remove_failed_item() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("a", None);
        let item = QueuedItem::new(id.clone(), test_completion_key(&id));
        queue.mark_failed(item, "error");

        assert!(queue.is_failed(&id));
        queue.remove(&id).unwrap();
        assert!(!queue.is_failed(&id));
    }

    #[test]
    fn test_remove_not_found() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("nonexistent", None);

        let result = queue.remove(&id);
        assert!(matches!(result, Err(DownloadError::NotInQueue { .. })));
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

    #[test]
    fn test_clear_failed() {
        let mut queue = DownloadQueue::new(10);
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);
        queue.mark_failed(
            QueuedItem::new(id_a.clone(), test_completion_key(&id_a)),
            "err1",
        );
        queue.mark_failed(
            QueuedItem::new(id_b.clone(), test_completion_key(&id_b)),
            "err2",
        );

        assert_eq!(queue.failed_len(), 2);

        queue.clear_failed();

        assert_eq!(queue.failed_len(), 0);
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
        let snapshot = queue.snapshot(None);
        assert_eq!(snapshot.items.len(), 3);
        assert_eq!(snapshot.items[0].position, 1);
        assert_eq!(snapshot.items[0].id, "a");
        assert_eq!(snapshot.items[1].position, 2);
        assert_eq!(snapshot.items[1].id, "b");
        assert_eq!(snapshot.items[2].position, 3);
        assert_eq!(snapshot.items[2].id, "c");

        // Dequeue "a"
        let dequeued = queue.dequeue().unwrap();
        assert_eq!(dequeued.id.model_id(), "a");

        // Snapshot again: positions should have shifted down
        let snapshot = queue.snapshot(None);
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.items[0].position, 1);
        assert_eq!(snapshot.items[0].id, "b");
        assert_eq!(snapshot.items[1].position, 2);
        assert_eq!(snapshot.items[1].id, "c");
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
        let snapshot = queue.snapshot(None);
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.active_count, 0);
        assert_eq!(snapshot.pending_count, 2);
        assert_eq!(snapshot.items[0].position, 1);
        assert_eq!(snapshot.items[0].id, "model-x");
        assert_eq!(snapshot.items[1].position, 2);
        assert_eq!(snapshot.items[1].id, "model-y");

        // Dequeue "model-x" and mark it as downloading
        let current = queue.dequeue().unwrap();
        assert_eq!(current.id.model_id(), "model-x");
        let current_dto = current.to_dto(1, DownloadStatus::Downloading);

        // Snapshot with active item: 1 active, 1 pending
        let snapshot = queue.snapshot(Some(current_dto));
        assert_eq!(snapshot.active_count, 1);
        assert_eq!(snapshot.pending_count, 1);
    }

    /// Test that `mark_failed()` moves an item to the failed list correctly.
    #[test]
    fn test_remove_item_moves_to_failed() {
        let mut queue = DownloadQueue::new(3);

        // Enqueue 2 items: "model-a" and "model-b"
        let id_a = test_id("model-a", None);
        let id_b = test_id("model-b", None);

        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        // Dequeue "model-a" then mark it as failed
        let item = queue.dequeue().unwrap();
        assert_eq!(item.id.model_id(), "model-a");

        let error_msg = "connection timeout";
        queue.mark_failed(item, error_msg);

        // Snapshot: only "model-b" remains in pending, failed item in recent_failures
        let snapshot = queue.snapshot(None);
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].id, "model-b");
        assert_eq!(snapshot.pending_count, 1);

        // Verify the failed item appears in recent_failures
        assert_eq!(snapshot.recent_failures.len(), 1);
        assert_eq!(snapshot.recent_failures[0].id, "model-a");
        assert_eq!(snapshot.recent_failures[0].error, error_msg);
    }

    /// Test that `clear_failed()` drains the failed list.
    #[test]
    fn test_clear_failed_drains_list() {
        let mut queue = DownloadQueue::new(5);

        // Enqueue 2 items: "a" and "b"
        let id_a = test_id("a", None);
        let id_b = test_id("b", None);

        queue
            .queue(id_a.clone(), test_completion_key(&id_a), false)
            .unwrap();
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        // Dequeue both, then mark each as failed with different errors
        let item_a = queue.dequeue().unwrap();
        let item_b = queue.dequeue().unwrap();

        queue.mark_failed(item_a, "error for a");
        queue.mark_failed(item_b, "error for b");

        // Verify 2 failed items
        assert_eq!(queue.failed_len(), 2);
        let snapshot = queue.snapshot(None);
        assert_eq!(snapshot.recent_failures.len(), 2);

        // Clear the failed list
        queue.clear_failed();

        // Verify list is drained
        assert_eq!(queue.failed_len(), 0);
        let snapshot = queue.snapshot(None);
        assert_eq!(snapshot.recent_failures.len(), 0);
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
            async { queue.read().await.snapshot(None) },
            async { queue.read().await.snapshot(None) },
            async { queue.read().await.snapshot(None) },
        );

        // All three snapshots should succeed and agree on queued items count
        assert_eq!(s1.items.len(), 2);
        assert_eq!(s2.items.len(), 2);
        assert_eq!(s3.items.len(), 2);

        // Each snapshot should have the same pending count
        assert_eq!(s1.pending_count, 2);
        assert_eq!(s2.pending_count, 2);
        assert_eq!(s3.pending_count, 2);
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
            let snapshot = guard.snapshot(None);
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
        assert_eq!(reader_snapshot.items.len(), 1);

        // Enqueuer completed successfully
        assert_eq!(enqueued_id.model_id(), "writer-item");

        // Final snapshot should show 2 items
        let final_snapshot = queue.read().await.snapshot(None);
        assert_eq!(final_snapshot.items.len(), 2);
        assert_eq!(final_snapshot.pending_count, 2);
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
