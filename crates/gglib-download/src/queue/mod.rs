//! Download queue management.
//!
//! This module provides a pure state machine for managing download queue state.
//! No I/O is performed here; the orchestrator (`DownloadManager`) handles I/O.
//!
//! # Design
//!
//! - Pure synchronous state machine (no async, no IO, no tracing)
//! - Commands produce events that the caller can use for side effects
//! - Deterministic: same inputs always produce same outputs
//!
//! # Position Semantics
//!
//! - Position 1 = currently downloading
//! - Position 2+ = waiting in queue
//! - Failed items have position 0 (not in active queue)

// Queue positions are always well under u32::MAX in practice
#![allow(clippy::cast_possible_truncation)]

mod shard_group;
mod types;

use std::collections::VecDeque;

use gglib_core::download::{
    CompletionKey, DownloadError, DownloadId, DownloadStatus, QueueSnapshot, ShardInfo,
};

pub use shard_group::ShardGroupId;
pub use types::{FailedItem, QueuedItem};

/// Manages the download queue state.
///
/// This is a sync type with no internal locking — the caller
/// (`DownloadManager`) is responsible for synchronization.
pub struct DownloadQueue {
    pending: VecDeque<QueuedItem>,
    failed: Vec<FailedItem>,
    max_size: u32,
}

impl DownloadQueue {
    /// Create a new download queue with the specified max size.
    pub const fn new(max_size: u32) -> Self {
        Self {
            pending: VecDeque::new(),
            failed: Vec::new(),
            max_size,
        }
    }

    /// Get the maximum queue size.
    pub const fn max_size(&self) -> u32 {
        self.max_size
    }

    /// Set the maximum queue size.
    pub const fn set_max_size(&mut self, size: u32) {
        self.max_size = size;
    }

    /// Get the number of pending items.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Get the number of failed items.
    #[cfg(test)]
    pub const fn failed_len(&self) -> usize {
        self.failed.len()
    }

    /// Check if a download ID is currently queued.
    pub fn is_queued(&self, id: &DownloadId) -> bool {
        self.pending.iter().any(|item| &item.id == id)
    }

    /// Check if a download ID is in the failed list.
    pub fn is_failed(&self, id: &DownloadId) -> bool {
        self.failed.iter().any(|item| &item.item.id == id)
    }

    /// Queue a single (non-sharded) download.
    ///
    /// Returns the 1-based queue position on success.
    ///
    /// Note: Production code uses `queue_sharded` for all downloads.
    /// This method is primarily for testing single-item queue behavior.
    #[cfg(test)]
    pub fn queue(
        &mut self,
        id: DownloadId,
        completion_key: CompletionKey,
        has_active: bool,
    ) -> Result<u32, DownloadError> {
        self.check_not_queued(&id)?;
        self.check_capacity(1)?;
        self.remove_from_failed(&id);

        let item = QueuedItem::new(id, completion_key);
        self.pending.push_back(item);

        // Position: if something is active, pending starts at 2
        let position = if has_active {
            self.pending.len() as u32 + 1
        } else {
            self.pending.len() as u32
        };

        Ok(position)
    }

    /// Queue a sharded download (multiple files with shared `group_id`).
    ///
    /// Returns the 1-based queue position of the first shard.
    pub fn queue_sharded(
        &mut self,
        id: &DownloadId,
        completion_key: &CompletionKey,
        shard_files: Vec<(String, Option<u64>)>,
        has_active: bool,
    ) -> Result<u32, DownloadError> {
        if shard_files.is_empty() {
            return Err(DownloadError::not_in_queue(id.to_string()));
        }

        self.check_not_queued(id)?;
        self.check_capacity(shard_files.len())?;
        self.remove_from_failed(id);

        let first_position = if has_active {
            self.pending.len() as u32 + 2
        } else {
            self.pending.len() as u32 + 1
        };

        let items = self.create_shard_items(id, completion_key, shard_files);
        self.pending.extend(items);

        Ok(first_position)
    }

    /// Pop the next item from the front of the queue.
    pub fn dequeue(&mut self) -> Option<QueuedItem> {
        self.pending.pop_front()
    }

    /// Clear all items from the queue (pending and failed).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.failed.clear();
    }

    /// Remove an item from the pending queue or failed list.
    pub fn remove(&mut self, id: &DownloadId) -> Result<(), DownloadError> {
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

    /// Reorder a queued item (or shard group) to a new position.
    ///
    /// Returns the actual 1-based position where the item(s) were placed.
    pub fn reorder(
        &mut self,
        id: &DownloadId,
        new_position: u32,
        has_active: bool,
    ) -> Result<u32, DownloadError> {
        // Find item(s) to move (handles shard groups)
        let group_id = self
            .pending
            .iter()
            .find(|item| &item.id == id)
            .and_then(|item| item.group_id.clone());

        let items_to_move: Vec<_> = if let Some(ref gid) = group_id {
            self.pending
                .iter()
                .filter(|item| item.group_id.as_ref() == Some(gid))
                .cloned()
                .collect()
        } else {
            self.pending
                .iter()
                .filter(|item| &item.id == id)
                .cloned()
                .collect()
        };

        if items_to_move.is_empty() {
            return Err(DownloadError::not_in_queue(id.to_string()));
        }

        // Remove then reinsert at new position
        if let Some(ref gid) = group_id {
            self.pending
                .retain(|item| item.group_id.as_ref() != Some(gid));
        } else {
            self.pending.retain(|item| &item.id != id);
        }

        // Convert 1-based position to 0-based index
        let target_index = if has_active {
            (new_position.saturating_sub(2)) as usize
        } else {
            (new_position.saturating_sub(1)) as usize
        };
        let insert_pos = target_index.min(self.pending.len());

        // Insert items at new position preserving order
        for (offset, item) in items_to_move.into_iter().enumerate() {
            let pos = insert_pos + offset;
            if pos >= self.pending.len() {
                self.pending.push_back(item);
            } else {
                self.pending.push_back(item);
                let len = self.pending.len();
                for i in (pos + 1..len).rev() {
                    self.pending.swap(i, i - 1);
                }
            }
        }

        // Return 1-based position
        let result_position = if has_active {
            insert_pos as u32 + 2
        } else {
            insert_pos as u32 + 1
        };

        Ok(result_position)
    }

    /// Get a snapshot of the current queue state for API responses.
    ///
    /// The `current_item` is the download currently being processed (if any).
    pub fn snapshot(
        &self,
        current_item: Option<gglib_core::download::QueuedDownload>,
    ) -> QueueSnapshot {
        let base_position = if current_item.is_some() { 2 } else { 1 };

        let pending: Vec<_> = self
            .pending
            .iter()
            .enumerate()
            .map(|(idx, item)| item.to_dto(base_position + idx as u32, DownloadStatus::Queued))
            .collect();

        let failed: Vec<_> = self.failed.iter().map(types::FailedItem::to_dto).collect();

        let active_count = u32::from(current_item.is_some());
        let pending_count = pending.len() as u32;

        let mut items = Vec::with_capacity(1 + pending.len());
        if let Some(current) = current_item {
            items.push(current);
        }
        items.extend(pending);

        QueueSnapshot {
            items,
            max_size: self.max_size,
            active_count,
            pending_count,
            recent_failures: failed,
        }
    }

    /// Mark a download as failed and add to the failed list.
    pub fn mark_failed(&mut self, item: QueuedItem, error: impl Into<String>) {
        self.failed.push(FailedItem::new(item, error));
    }

    /// Clear all failed downloads.
    pub fn clear_failed(&mut self) {
        self.failed.clear();
    }

    /// Retry a failed download by moving it back to the pending queue.
    ///
    /// Returns the 1-based position in the queue on success.
    pub fn retry_failed(
        &mut self,
        id: &DownloadId,
        has_active: bool,
    ) -> Result<u32, DownloadError> {
        // Find and remove from failed list
        let pos = self.failed.iter().position(|f| &f.item.id == id);
        let failed = match pos {
            Some(idx) => self.failed.remove(idx),
            None => return Err(DownloadError::not_in_queue(id.to_string())),
        };

        // Add back to pending queue with fresh timestamp, reusing completion_key
        let item = QueuedItem::new(failed.item.id, failed.item.completion_key);
        self.pending.push_back(item);

        // Return 1-based position
        let position = if has_active {
            self.pending.len() as u32 + 1
        } else {
            self.pending.len() as u32
        };

        Ok(position)
    }

    // --- Shard group helpers ---

    /// Remove all pending items belonging to a shard group.
    pub fn remove_group(&mut self, group_id: &ShardGroupId) -> usize {
        let initial = self.pending.len();
        self.pending
            .retain(|item| item.group_id.as_ref() != Some(group_id));
        initial - self.pending.len()
    }

    /// Move all pending items in a shard group to the failed list.
    #[cfg(test)]
    pub fn fail_group(&mut self, group_id: &ShardGroupId, error: &str) -> usize {
        let mut removed = Vec::new();
        self.pending.retain(|item| {
            if item.group_id.as_ref() == Some(group_id) {
                removed.push(item.clone());
                false
            } else {
                true
            }
        });
        let count = removed.len();
        for item in removed {
            self.failed.push(FailedItem::new(item, error));
        }
        count
    }

    // --- Private helpers ---

    fn check_not_queued(&self, id: &DownloadId) -> Result<(), DownloadError> {
        if self.is_queued(id) {
            Err(DownloadError::already_queued(id.to_string()))
        } else {
            Ok(())
        }
    }

    fn check_capacity(&self, additional: usize) -> Result<(), DownloadError> {
        if self.pending.len() + additional > self.max_size as usize {
            Err(DownloadError::queue_full(self.max_size))
        } else {
            Ok(())
        }
    }

    fn remove_from_failed(&mut self, id: &DownloadId) {
        self.failed.retain(|item| &item.item.id != id);
    }

    #[allow(clippy::unused_self)]
    fn create_shard_items(
        &self,
        id: &DownloadId,
        completion_key: &CompletionKey,
        shard_files: Vec<(String, Option<u64>)>,
    ) -> Vec<QueuedItem> {
        let group_id = ShardGroupId::generate(id);
        let total_shards = shard_files.len() as u32;

        shard_files
            .into_iter()
            .enumerate()
            .map(|(idx, (filename, size))| {
                let shard_info = match size {
                    Some(s) => ShardInfo::with_size(idx as u32, total_shards, filename, s),
                    None => ShardInfo::new(idx as u32, total_shards, filename),
                };
                QueuedItem::new_shard(
                    id.clone(),
                    group_id.clone(),
                    shard_info,
                    completion_key.clone(),
                )
            })
            .collect()
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
            ("shard-001.gguf".to_string(), Some(1000u64)),
            ("shard-002.gguf".to_string(), Some(2000u64)),
        ];

        let key = test_completion_key(&id);
        let pos = queue.queue_sharded(&id, &key, shards, false).unwrap();
        assert_eq!(pos, 1);
        assert_eq!(queue.pending_len(), 2);

        // All items share the same group_id
        let snapshot = queue.snapshot(None);
        let group_id = snapshot.items[0].group_id.as_ref().unwrap();
        assert_eq!(snapshot.items[1].group_id.as_ref().unwrap(), group_id);
    }

    #[test]
    fn test_remove_group() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/x", Some("Q4_K_M"));
        let shards = vec![("s1.gguf".to_string(), None), ("s2.gguf".to_string(), None)];
        let key = test_completion_key(&id);
        queue.queue_sharded(&id, &key, shards, false).unwrap();

        let group_id = queue.pending.front().unwrap().group_id.clone().unwrap();
        let removed = queue.remove_group(&group_id);

        assert_eq!(removed, 2);
        assert!(queue.pending.is_empty());
    }

    #[test]
    fn test_fail_group() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/x", Some("Q4_K_M"));
        let shards = vec![("s1.gguf".to_string(), None), ("s2.gguf".to_string(), None)];
        let key = test_completion_key(&id);
        queue.queue_sharded(&id, &key, shards, false).unwrap();

        let group_id = queue.pending.front().unwrap().group_id.clone().unwrap();
        let failed_count = queue.fail_group(&group_id, "Network error");

        assert_eq!(failed_count, 2);
        assert!(queue.pending.is_empty());
        assert_eq!(queue.failed.len(), 2);
        assert_eq!(queue.failed[0].error, "Network error");
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
        let new_pos = queue.reorder(&test_id("c", None), 1, false).unwrap();

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
        let new_pos = queue.reorder(&test_id("c", None), 2, false).unwrap();

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

        // has_active = true, so position 1 is the active item
        // Move "c" to position 2 (first pending slot)
        let new_pos = queue.reorder(&test_id("c", None), 2, true).unwrap();

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

        let result = queue.reorder(&test_id("nonexistent", None), 1, false);
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
        let shards = vec![("s1.gguf".to_string(), None), ("s2.gguf".to_string(), None)];
        queue
            .queue_sharded(
                &id_sharded.clone(),
                &test_completion_key(&id_sharded),
                shards,
                false,
            )
            .unwrap();

        let id_b = test_id("b", None);
        queue
            .queue(id_b.clone(), test_completion_key(&id_b), false)
            .unwrap();

        // Move shard group to front - both shards should move together
        let new_pos = queue.reorder(&id_sharded, 1, false).unwrap();

        assert_eq!(new_pos, 1);
        let ids: Vec<_> = queue.pending.iter().map(|i| i.id.model_id()).collect();
        // Both shards at front, then a, then b
        assert_eq!(ids, vec!["sharded", "sharded", "a", "b"]);
    }

    #[test]
    fn test_retry_failed() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("failed-model", None);
        let item = QueuedItem::new(id.clone(), test_completion_key(&id));
        queue.mark_failed(item, "temporary error");

        assert!(queue.is_failed(&id));
        assert!(!queue.is_queued(&id));

        let position = queue.retry_failed(&id, false).unwrap();

        assert_eq!(position, 1);
        assert!(!queue.is_failed(&id));
        assert!(queue.is_queued(&id));
    }

    #[test]
    fn test_retry_failed_with_existing_queue() {
        let mut queue = DownloadQueue::new(10);
        let id_existing = test_id("existing", None);
        queue
            .queue(
                id_existing.clone(),
                test_completion_key(&id_existing),
                false,
            )
            .unwrap();

        let id = test_id("failed-model", None);
        let item = QueuedItem::new(id.clone(), test_completion_key(&id));
        queue.mark_failed(item, "error");

        let position = queue.retry_failed(&id, false).unwrap();

        // Should be at end of queue
        assert_eq!(position, 2);
        assert_eq!(queue.pending_len(), 2);
    }

    #[test]
    fn test_retry_failed_not_found() {
        let mut queue = DownloadQueue::new(10);

        let result = queue.retry_failed(&test_id("nonexistent", None), false);
        assert!(matches!(result, Err(DownloadError::NotInQueue { .. })));
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
}
