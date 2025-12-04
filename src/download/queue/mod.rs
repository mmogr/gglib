//! Download queue management.
//!
//! This module provides a pure state machine for managing download queue state.
//! No I/O is performed here; the orchestrator (`DownloadManager`) handles I/O.

mod shard_group;
mod types;

use crate::download::domain::errors::DownloadError;
use crate::download::domain::events::{DownloadStatus, DownloadSummary};
use crate::download::domain::types::{DownloadId, ShardInfo};
use std::collections::VecDeque;

pub use shard_group::ShardGroupId;
pub use types::{FailedDownload, QueueSnapshot, QueuedDownload};

/// Manages the download queue state.
///
/// This is a sync type with no internal locking — the caller
/// (`DownloadManager`) is responsible for synchronization.
///
/// # Position Semantics
///
/// - Position 1 = currently downloading
/// - Position 2+ = waiting in queue
/// - Failed items have position 0 (not in active queue)
pub struct DownloadQueue {
    pending: VecDeque<QueuedDownload>,
    failed: Vec<FailedDownload>,
    max_size: u32,
}

impl DownloadQueue {
    /// Create a new download queue with the specified max size.
    pub fn new(max_size: u32) -> Self {
        Self {
            pending: VecDeque::new(),
            failed: Vec::new(),
            max_size,
        }
    }

    /// Get the maximum queue size.
    pub fn max_size(&self) -> u32 {
        self.max_size
    }

    /// Set the maximum queue size.
    pub fn set_max_size(&mut self, size: u32) {
        self.max_size = size;
    }

    /// Get the number of pending items.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Get the number of failed items.
    pub fn failed_len(&self) -> usize {
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
    pub fn queue(&mut self, id: DownloadId, has_active: bool) -> Result<u32, DownloadError> {
        self.check_not_queued(&id)?;
        self.check_capacity(1)?;
        self.remove_from_failed(&id);

        let item = QueuedDownload::new(id);
        self.pending.push_back(item);

        // Position: if something is active, pending starts at 2
        let position = if has_active {
            self.pending.len() as u32 + 1
        } else {
            self.pending.len() as u32
        };

        Ok(position)
    }

    /// Queue a sharded download (multiple files with shared group_id).
    ///
    /// Returns the 1-based queue position of the first shard.
    pub fn queue_sharded(
        &mut self,
        id: DownloadId,
        shard_files: Vec<(String, Option<u64>)>,
        has_active: bool,
    ) -> Result<u32, DownloadError> {
        if shard_files.is_empty() {
            return Err(DownloadError::not_in_queue(id.to_string()));
        }

        self.check_not_queued(&id)?;
        self.check_capacity(shard_files.len())?;
        self.remove_from_failed(&id);

        let first_position = if has_active {
            self.pending.len() as u32 + 2
        } else {
            self.pending.len() as u32 + 1
        };

        let items = self.create_shard_items(&id, shard_files);
        self.pending.extend(items);

        Ok(first_position)
    }

    /// Pop the next item from the front of the queue (alias for pop_next).
    pub fn dequeue(&mut self) -> Option<QueuedDownload> {
        self.pop_next()
    }

    /// Pop the next item from the front of the queue.
    pub fn pop_next(&mut self) -> Option<QueuedDownload> {
        self.pending.pop_front()
    }

    /// Peek at the next item without removing it.
    pub fn peek_next(&self) -> Option<&QueuedDownload> {
        self.pending.front()
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

    /// Get a snapshot of the current queue state.
    ///
    /// The `current` field is None here; the caller provides it separately.
    pub fn snapshot(&self, current: Option<DownloadSummary>) -> QueueSnapshot {
        let base_position = if current.is_some() { 2 } else { 1 };

        let pending: Vec<_> = self
            .pending
            .iter()
            .enumerate()
            .map(|(idx, item)| item.to_summary(base_position + idx as u32, DownloadStatus::Queued))
            .collect();

        let failed: Vec<_> = self
            .failed
            .iter()
            .enumerate()
            .map(|(idx, item)| item.to_summary(idx as u32))
            .collect();

        QueueSnapshot {
            current,
            pending,
            failed,
            max_size: self.max_size,
        }
    }

    /// Mark a download as failed and add to the failed list.
    pub fn mark_failed(&mut self, failed: FailedDownload) {
        self.failed.push(failed);
    }

    /// Clear all failed downloads.
    pub fn clear_failed(&mut self) {
        self.failed.clear();
    }

    /// Get a failed download by ID (for retry).
    pub fn get_failed(&self, id: &DownloadId) -> Option<&FailedDownload> {
        self.failed.iter().find(|f| &f.item.id == id)
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

        // Add back to pending queue
        self.pending.push_back(failed.item);

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
    pub fn fail_group(&mut self, group_id: &ShardGroupId, error: &str) -> usize {
        let mut removed = Vec::new();
        self.pending.retain(|item| {
            if item.group_id.as_ref() == Some(group_id) {
                removed.push(FailedDownload::new(item.clone(), error));
                false
            } else {
                true
            }
        });
        let count = removed.len();
        self.failed.extend(removed);
        count
    }

    /// Check if there are no more pending items in a shard group.
    pub fn is_last_in_group(&self, group_id: &ShardGroupId) -> bool {
        !self
            .pending
            .iter()
            .any(|item| item.group_id.as_ref() == Some(group_id))
    }

    /// Get the total remaining size of pending shards in a group.
    pub fn group_remaining_size(&self, group_id: &ShardGroupId) -> u64 {
        self.pending
            .iter()
            .filter(|item| item.group_id.as_ref() == Some(group_id))
            .filter_map(|item| item.shard_info.as_ref())
            .filter_map(|shard| shard.file_size)
            .sum()
    }

    /// Count pending items in a shard group.
    pub fn group_pending_count(&self, group_id: &ShardGroupId) -> usize {
        self.pending
            .iter()
            .filter(|item| item.group_id.as_ref() == Some(group_id))
            .count()
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

    fn create_shard_items(
        &self,
        id: &DownloadId,
        shard_files: Vec<(String, Option<u64>)>,
    ) -> Vec<QueuedDownload> {
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
                QueuedDownload::new_shard(id.clone(), group_id.clone(), shard_info)
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

    #[test]
    fn test_queue_single_download() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/a", Some("Q4_K_M"));
        let pos = queue.queue(id.clone(), false).unwrap();

        assert_eq!(pos, 1); // 1-based, no active
        assert!(queue.is_queued(&id));
        assert_eq!(queue.pending_len(), 1);
    }

    #[test]
    fn test_queue_with_active() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/a", None);
        let pos = queue.queue(id, true).unwrap(); // has_active = true

        assert_eq!(pos, 2); // Position 1 is active, so this is 2
    }

    #[test]
    fn test_queue_multiple() {
        let mut queue = DownloadQueue::new(10);
        let pos1 = queue.queue(test_id("a", None), false).unwrap();
        let pos2 = queue.queue(test_id("b", None), false).unwrap();
        let pos3 = queue.queue(test_id("c", None), false).unwrap();

        assert_eq!(pos1, 1);
        assert_eq!(pos2, 2);
        assert_eq!(pos3, 3);
    }

    #[test]
    fn test_queue_rejects_duplicate() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/a", None);
        queue.queue(id.clone(), false).unwrap();

        let result = queue.queue(id, false);
        assert!(matches!(result, Err(DownloadError::AlreadyQueued { .. })));
    }

    #[test]
    fn test_queue_respects_capacity() {
        let mut queue = DownloadQueue::new(2);
        queue.queue(test_id("a", None), false).unwrap();
        queue.queue(test_id("b", None), false).unwrap();

        let result = queue.queue(test_id("c", None), false);
        assert!(matches!(
            result,
            Err(DownloadError::QueueFull { max_size: 2 })
        ));
    }

    #[test]
    fn test_pop_next_fifo() {
        let mut queue = DownloadQueue::new(10);
        queue.queue(test_id("a", None), false).unwrap();
        queue.queue(test_id("b", None), false).unwrap();

        let first = queue.pop_next().unwrap();
        assert_eq!(first.id.model_id(), "a");

        let second = queue.pop_next().unwrap();
        assert_eq!(second.id.model_id(), "b");

        assert!(queue.pop_next().is_none());
    }

    #[test]
    fn test_snapshot_positions() {
        let mut queue = DownloadQueue::new(10);
        queue.queue(test_id("a", None), false).unwrap();
        queue.queue(test_id("b", None), false).unwrap();

        // Simulate "a" is now active
        let current = queue
            .pop_next()
            .unwrap()
            .to_summary(1, DownloadStatus::Downloading);
        let snapshot = queue.snapshot(Some(current));

        assert!(snapshot.current.is_some());
        assert_eq!(snapshot.current.as_ref().unwrap().position, 1);
        assert_eq!(snapshot.pending.len(), 1);
        assert_eq!(snapshot.pending[0].position, 2); // Because current is at 1
    }

    #[test]
    fn test_sharded_download() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/x", Some("Q4_K_M"));
        let shards = vec![
            ("shard-001.gguf".to_string(), Some(1000u64)),
            ("shard-002.gguf".to_string(), Some(2000u64)),
        ];

        let pos = queue.queue_sharded(id, shards, false).unwrap();
        assert_eq!(pos, 1);
        assert_eq!(queue.pending_len(), 2);

        // All items share the same group_id
        let snapshot = queue.snapshot(None);
        let group_id = snapshot.pending[0].group_id.as_ref().unwrap();
        assert_eq!(snapshot.pending[1].group_id.as_ref().unwrap(), group_id);
    }

    #[test]
    fn test_remove_group() {
        let mut queue = DownloadQueue::new(10);
        let id = test_id("model/x", Some("Q4_K_M"));
        let shards = vec![("s1.gguf".to_string(), None), ("s2.gguf".to_string(), None)];
        queue.queue_sharded(id, shards, false).unwrap();

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
        queue.queue_sharded(id, shards, false).unwrap();

        let group_id = queue.pending.front().unwrap().group_id.clone().unwrap();
        let failed_count = queue.fail_group(&group_id, "Network error");

        assert_eq!(failed_count, 2);
        assert!(queue.pending.is_empty());
        assert_eq!(queue.failed.len(), 2);
        assert_eq!(queue.failed[0].error, "Network error");
    }
}
