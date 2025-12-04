//! Download queue management.
//!
//! Sync data structure for managing pending and failed downloads.
//! Wrapped in a lock by `DownloadService` for async access.
//!
//! # Design
//!
//! - `DownloadQueue` is a **sync** type with no internal locking.
//! - The caller (`DownloadService`) wraps it in `Arc<RwLock<_>>` for async access.
//! - Internal indices are **0-based**; the caller converts to UI-friendly positions.
//! - `max_size` bounds the **pending queue only**, not including active downloads.

use super::download_models::{
    DownloadError, DownloadQueueItem, DownloadQueueStatus, DownloadStatus, QueuedDownload,
    ShardInfo,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Newtype for shard group identifiers.
///
/// Groups multiple shard downloads together for coordinated operations
/// (cancel all, fail all, retry all).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShardGroupId(pub String);

impl ShardGroupId {
    /// Create a new shard group ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a unique group ID for a sharded download.
    pub fn generate(model_id: &str, quantization: &str) -> Self {
        Self(format!(
            "{}:{}:{}",
            model_id,
            quantization,
            uuid::Uuid::new_v4()
        ))
    }

    /// Get the inner string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ShardGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ShardGroupId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ShardGroupId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A failed download with error information.
///
/// Wraps `QueuedDownload` with the failure reason so the UI
/// can display why something failed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailedDownload {
    /// The original queued download item.
    pub item: QueuedDownload,
    /// Human-readable error message.
    pub error: String,
}

impl FailedDownload {
    /// Create a new failed download entry.
    pub fn new(mut item: QueuedDownload, error: impl Into<String>) -> Self {
        item.queued_at = None; // Clear the queued timestamp
        Self {
            item,
            error: error.into(),
        }
    }

    /// Get the model ID.
    pub fn model_id(&self) -> &str {
        &self.item.model_id
    }

    /// Get the group ID if this was a sharded download.
    pub fn group_id(&self) -> Option<&ShardGroupId> {
        self.item.group_id.as_ref().map(|s| {
            // SAFETY: We're just reinterpreting the string as ShardGroupId
            // This is a bit awkward but avoids changing QueuedDownload's type
            unsafe { &*(s as *const String as *const ShardGroupId) }
        })
    }
}

/// Manages the download queue state.
///
/// This is a sync type with no internal locking — the caller
/// (`DownloadService`) is responsible for synchronization.
///
/// # Position Semantics
///
/// - All positions returned by this struct are **0-based** indices into the pending queue.
/// - The caller is responsible for converting to UI-friendly 1-based positions
///   and accounting for the "current download" slot.
///
/// # Capacity
///
/// - `max_size` bounds the **pending queue only**.
/// - Active downloads are tracked separately by the caller.
/// - `queue()` methods require the caller to pass in the current `active_count`.
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

    /// Check if a model is currently queued (in pending queue).
    pub fn is_queued(&self, model_id: &str) -> bool {
        self.pending.iter().any(|item| item.model_id == model_id)
    }

    /// Check if a model is in the failed list.
    pub fn is_failed(&self, model_id: &str) -> bool {
        self.failed
            .iter()
            .any(|item| item.item.model_id == model_id)
    }

    /// Queue a single (non-sharded) download.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `quantization` - Optional quantization type
    /// * `active_count` - Number of currently active downloads (for capacity check message)
    ///
    /// # Returns
    ///
    /// Returns the 0-based queue position on success.
    ///
    /// # Errors
    ///
    /// - `AlreadyRunning` if the model is already queued
    /// - `QueueFull` if adding would exceed `max_size`
    pub fn queue(
        &mut self,
        model_id: String,
        quantization: Option<String>,
        active_count: usize,
    ) -> Result<usize, DownloadError> {
        self.check_not_queued(&model_id)?;
        self.check_capacity(active_count, 1)?;
        self.remove_from_failed(&model_id);

        let item = QueuedDownload::new(model_id, quantization);
        self.pending.push_back(item);
        Ok(self.pending.len() - 1) // 0-based index
    }

    /// Queue a sharded download (multiple files with shared group_id).
    ///
    /// Each shard is queued as a separate item. All shards share the same
    /// `group_id` for coordinated operations.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `quantization` - Quantization type
    /// * `shard_files` - List of (filename, optional size) tuples
    /// * `active_count` - Number of currently active downloads
    ///
    /// # Returns
    ///
    /// Returns the 0-based queue position of the first shard.
    pub fn queue_sharded(
        &mut self,
        model_id: String,
        quantization: String,
        shard_files: Vec<(String, Option<u64>)>,
        active_count: usize,
    ) -> Result<usize, DownloadError> {
        if shard_files.is_empty() {
            return Err(DownloadError::NotInQueue {
                model_id: model_id.clone(),
            });
        }

        self.check_not_queued(&model_id)?;
        self.check_capacity(active_count, shard_files.len())?;
        self.remove_from_failed(&model_id);

        let first_position = self.pending.len();
        let items = self.create_shard_items(&model_id, &quantization, shard_files);
        self.pending.extend(items);

        Ok(first_position)
    }

    /// Pop the next item from the front of the queue.
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop_next(&mut self) -> Option<QueuedDownload> {
        self.pending.pop_front()
    }

    /// Peek at the next item without removing it.
    pub fn peek_next(&self) -> Option<&QueuedDownload> {
        self.pending.front()
    }

    /// Remove an item from the pending queue or failed list.
    ///
    /// # Errors
    ///
    /// Returns `NotInQueue` if the model is not found in either list.
    pub fn remove(&mut self, model_id: &str) -> Result<(), DownloadError> {
        let initial_pending = self.pending.len();
        self.pending.retain(|item| item.model_id != model_id);

        if self.pending.len() < initial_pending {
            return Ok(());
        }

        let initial_failed = self.failed.len();
        self.failed.retain(|item| item.item.model_id != model_id);

        if self.failed.len() < initial_failed {
            Ok(())
        } else {
            Err(DownloadError::NotInQueue {
                model_id: model_id.to_string(),
            })
        }
    }

    /// Reorder a queued item (or shard group) to a new position.
    ///
    /// For sharded models, all shards with the same `group_id` are moved
    /// together, preserving their relative order.
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model ID to move
    /// * `new_position` - Target 0-based position in the pending queue
    ///
    /// # Returns
    ///
    /// The actual 0-based position where the item(s) were placed.
    pub fn reorder(&mut self, model_id: &str, new_position: usize) -> Result<usize, DownloadError> {
        // Find item(s) to move (handles shard groups)
        let group_id = self
            .pending
            .iter()
            .find(|item| item.model_id == model_id)
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
                .filter(|item| item.model_id == model_id)
                .cloned()
                .collect()
        };

        if items_to_move.is_empty() {
            return Err(DownloadError::NotInQueue {
                model_id: model_id.to_string(),
            });
        }

        // Remove then reinsert at new position
        if let Some(ref gid) = group_id {
            self.pending
                .retain(|item| item.group_id.as_ref() != Some(gid));
        } else {
            self.pending.retain(|item| item.model_id != model_id);
        }

        let insert_pos = new_position.min(self.pending.len());

        // Insert items at new position preserving order
        for (offset, item) in items_to_move.into_iter().enumerate() {
            let pos = insert_pos + offset;
            if pos >= self.pending.len() {
                self.pending.push_back(item);
            } else {
                // VecDeque doesn't have insert, so we use a workaround
                self.pending.push_back(item);
                // Rotate the last element to the target position
                let len = self.pending.len();
                for i in (pos + 1..len).rev() {
                    self.pending.swap(i, i - 1);
                }
            }
        }

        Ok(insert_pos)
    }

    /// Get the current queue status.
    ///
    /// # Arguments
    ///
    /// * `current` - The currently downloading item (if any), provided by caller
    ///
    /// # Returns
    ///
    /// A `DownloadQueueStatus` with UI-facing positions for pending items.
    /// Positions start at 2 (position 1 is reserved for current download).
    pub fn status(&self, current: Option<DownloadQueueItem>) -> DownloadQueueStatus {
        // Position 1 is reserved for current download, pending starts at 2
        let offset = 2;

        let pending_items: Vec<_> = self
            .pending
            .iter()
            .enumerate()
            .map(|(idx, item)| DownloadQueueItem {
                model_id: item.model_id.clone(),
                quantization: item.quantization.clone(),
                status: DownloadStatus::Queued,
                position: idx + offset, // UI-facing position (starts at 2)
                error: None,
                group_id: item.group_id.clone(),
                shard_info: item.shard_info.clone(),
            })
            .collect();

        let failed_items: Vec<_> = self
            .failed
            .iter()
            .enumerate()
            .map(|(idx, failed)| DownloadQueueItem {
                model_id: failed.item.model_id.clone(),
                quantization: failed.item.quantization.clone(),
                status: DownloadStatus::Failed,
                position: idx,
                error: Some(failed.error.clone()),
                group_id: failed.item.group_id.clone(),
                shard_info: failed.item.shard_info.clone(),
            })
            .collect();

        DownloadQueueStatus {
            current,
            pending: pending_items,
            failed: failed_items,
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

    /// Get a failed download by model ID (for retry).
    pub fn get_failed(&self, model_id: &str) -> Option<&FailedDownload> {
        self.failed.iter().find(|f| f.item.model_id == model_id)
    }

    // --- Shard group helpers ---

    /// Remove all pending items belonging to a shard group.
    ///
    /// Returns the number of items removed.
    pub fn remove_group(&mut self, group_id: &ShardGroupId) -> usize {
        let initial = self.pending.len();
        self.pending
            .retain(|item| item.group_id.as_deref() != Some(group_id.as_str()));
        initial - self.pending.len()
    }

    /// Move all pending items in a shard group to the failed list.
    ///
    /// Returns the number of items moved.
    pub fn fail_group(&mut self, group_id: &ShardGroupId, error: &str) -> usize {
        let mut removed = Vec::new();
        self.pending.retain(|item| {
            if item.group_id.as_deref() == Some(group_id.as_str()) {
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
    ///
    /// Returns `true` if the group has no remaining pending items,
    /// meaning the current item is the last one.
    pub fn is_last_in_group(&self, group_id: &ShardGroupId) -> bool {
        !self
            .pending
            .iter()
            .any(|item| item.group_id.as_deref() == Some(group_id.as_str()))
    }

    /// Get the total remaining size of pending shards in a group.
    ///
    /// Returns the sum of `file_size` for all pending items in the group.
    pub fn group_remaining_size(&self, group_id: &ShardGroupId) -> u64 {
        self.pending
            .iter()
            .filter(|item| item.group_id.as_deref() == Some(group_id.as_str()))
            .filter_map(|item| item.shard_info.as_ref())
            .filter_map(|shard| shard.file_size)
            .sum()
    }

    /// Count pending items in a shard group.
    pub fn group_pending_count(&self, group_id: &ShardGroupId) -> usize {
        self.pending
            .iter()
            .filter(|item| item.group_id.as_deref() == Some(group_id.as_str()))
            .count()
    }

    // --- Private helpers ---

    fn check_not_queued(&self, model_id: &str) -> Result<(), DownloadError> {
        if self.is_queued(model_id) {
            Err(DownloadError::AlreadyRunning {
                model_id: model_id.to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn check_capacity(&self, active_count: usize, additional: usize) -> Result<(), DownloadError> {
        // max_size bounds pending queue only, but we include active in the error message
        if self.pending.len() + additional > self.max_size as usize {
            Err(DownloadError::QueueFull {
                max_size: self.max_size,
            })
        } else {
            // Suppress unused variable warning - active_count kept for API consistency
            let _ = active_count;
            Ok(())
        }
    }

    fn remove_from_failed(&mut self, model_id: &str) {
        self.failed.retain(|item| item.item.model_id != model_id);
    }

    fn create_shard_items(
        &self,
        model_id: &str,
        quantization: &str,
        shard_files: Vec<(String, Option<u64>)>,
    ) -> Vec<QueuedDownload> {
        let group_id = ShardGroupId::generate(model_id, quantization);
        let total_shards = shard_files.len();

        shard_files
            .into_iter()
            .enumerate()
            .map(|(idx, (filename, size))| {
                let shard_info = match size {
                    Some(s) => ShardInfo::with_size(idx, total_shards, filename, s),
                    None => ShardInfo::new(idx, total_shards, filename),
                };
                QueuedDownload::new_shard(
                    model_id.to_string(),
                    quantization.to_string(),
                    group_id.0.clone(),
                    shard_info,
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

    #[test]
    fn test_queue_single_download() {
        let mut queue = DownloadQueue::new(10);
        let pos = queue
            .queue("model/a".into(), Some("Q4_K_M".into()), 0)
            .unwrap();
        assert_eq!(pos, 0); // 0-based
        assert!(queue.is_queued("model/a"));
        assert_eq!(queue.pending_len(), 1);
    }

    #[test]
    fn test_queue_multiple_downloads() {
        let mut queue = DownloadQueue::new(10);
        let pos1 = queue.queue("model/a".into(), None, 0).unwrap();
        let pos2 = queue.queue("model/b".into(), None, 0).unwrap();
        let pos3 = queue.queue("model/c".into(), None, 0).unwrap();

        assert_eq!(pos1, 0);
        assert_eq!(pos2, 1);
        assert_eq!(pos3, 2);
        assert_eq!(queue.pending_len(), 3);
    }

    #[test]
    fn test_queue_rejects_duplicate() {
        let mut queue = DownloadQueue::new(10);
        queue.queue("model/a".into(), None, 0).unwrap();
        let result = queue.queue("model/a".into(), None, 0);
        assert!(matches!(result, Err(DownloadError::AlreadyRunning { .. })));
    }

    #[test]
    fn test_queue_respects_capacity() {
        let mut queue = DownloadQueue::new(2);
        queue.queue("model/a".into(), None, 0).unwrap();
        queue.queue("model/b".into(), None, 0).unwrap();
        let result = queue.queue("model/c".into(), None, 0);
        assert!(matches!(
            result,
            Err(DownloadError::QueueFull { max_size: 2 })
        ));
    }

    #[test]
    fn test_pop_next_fifo() {
        let mut queue = DownloadQueue::new(10);
        queue.queue("model/a".into(), None, 0).unwrap();
        queue.queue("model/b".into(), None, 0).unwrap();

        let first = queue.pop_next().unwrap();
        assert_eq!(first.model_id, "model/a");

        let second = queue.pop_next().unwrap();
        assert_eq!(second.model_id, "model/b");

        assert!(queue.pop_next().is_none());
    }

    #[test]
    fn test_peek_next() {
        let mut queue = DownloadQueue::new(10);
        queue.queue("model/a".into(), None, 0).unwrap();

        let peeked = queue.peek_next().unwrap();
        assert_eq!(peeked.model_id, "model/a");

        // Still there after peek
        assert_eq!(queue.pending_len(), 1);
    }

    #[test]
    fn test_remove_from_pending() {
        let mut queue = DownloadQueue::new(10);
        queue.queue("model/a".into(), None, 0).unwrap();
        queue.queue("model/b".into(), None, 0).unwrap();

        queue.remove("model/a").unwrap();
        assert!(!queue.is_queued("model/a"));
        assert!(queue.is_queued("model/b"));
        assert_eq!(queue.pending_len(), 1);
    }

    #[test]
    fn test_remove_from_failed() {
        let mut queue = DownloadQueue::new(10);
        let item = QueuedDownload::new("model/a".into(), None);
        queue.mark_failed(FailedDownload::new(item, "test error"));

        assert!(queue.is_failed("model/a"));
        queue.remove("model/a").unwrap();
        assert!(!queue.is_failed("model/a"));
    }

    #[test]
    fn test_remove_not_found() {
        let mut queue = DownloadQueue::new(10);
        let result = queue.remove("nonexistent");
        assert!(matches!(result, Err(DownloadError::NotInQueue { .. })));
    }

    #[test]
    fn test_sharded_download_creates_group() {
        let mut queue = DownloadQueue::new(10);
        let shards = vec![
            ("shard-001.gguf".into(), Some(1000u64)),
            ("shard-002.gguf".into(), Some(1000u64)),
        ];
        let pos = queue
            .queue_sharded("model/x".into(), "Q4_K_M".into(), shards, 0)
            .unwrap();

        assert_eq!(pos, 0);
        assert_eq!(queue.pending_len(), 2);

        let status = queue.status(None);
        assert_eq!(status.pending.len(), 2);

        // All items share the same group_id
        let group_id = status.pending[0].group_id.as_ref().unwrap();
        assert_eq!(status.pending[1].group_id.as_ref().unwrap(), group_id);

        // Group ID contains model and quantization
        assert!(group_id.contains("model/x"));
        assert!(group_id.contains("Q4_K_M"));
    }

    #[test]
    fn test_sharded_download_has_shard_info() {
        let mut queue = DownloadQueue::new(10);
        let shards = vec![
            ("shard-001.gguf".into(), Some(1000u64)),
            ("shard-002.gguf".into(), Some(2000u64)),
        ];
        queue
            .queue_sharded("model/x".into(), "Q4_K_M".into(), shards, 0)
            .unwrap();

        let status = queue.status(None);

        let shard0 = status.pending[0].shard_info.as_ref().unwrap();
        assert_eq!(shard0.shard_index, 0);
        assert_eq!(shard0.total_shards, 2);
        assert_eq!(shard0.filename, "shard-001.gguf");
        assert_eq!(shard0.file_size, Some(1000));

        let shard1 = status.pending[1].shard_info.as_ref().unwrap();
        assert_eq!(shard1.shard_index, 1);
        assert_eq!(shard1.total_shards, 2);
        assert_eq!(shard1.filename, "shard-002.gguf");
        assert_eq!(shard1.file_size, Some(2000));
    }

    #[test]
    fn test_remove_group() {
        let mut queue = DownloadQueue::new(10);
        let shards = vec![
            ("shard-001.gguf".into(), None),
            ("shard-002.gguf".into(), None),
        ];
        queue
            .queue_sharded("model/x".into(), "Q4_K_M".into(), shards, 0)
            .unwrap();

        let group_id: ShardGroupId = queue
            .pending
            .front()
            .unwrap()
            .group_id
            .clone()
            .unwrap()
            .into();

        let removed = queue.remove_group(&group_id);
        assert_eq!(removed, 2);
        assert!(queue.pending.is_empty());
    }

    #[test]
    fn test_fail_group_moves_all_shards() {
        let mut queue = DownloadQueue::new(10);
        let shards = vec![
            ("shard-001.gguf".into(), None),
            ("shard-002.gguf".into(), None),
        ];
        queue
            .queue_sharded("model/x".into(), "Q4_K_M".into(), shards, 0)
            .unwrap();

        let group_id: ShardGroupId = queue
            .pending
            .front()
            .unwrap()
            .group_id
            .clone()
            .unwrap()
            .into();

        let failed_count = queue.fail_group(&group_id, "Download failed");
        assert_eq!(failed_count, 2);
        assert!(queue.pending.is_empty());
        assert_eq!(queue.failed.len(), 2);

        // Check error is preserved
        assert_eq!(queue.failed[0].error, "Download failed");
        assert_eq!(queue.failed[1].error, "Download failed");
    }

    #[test]
    fn test_is_last_in_group() {
        let mut queue = DownloadQueue::new(10);
        let shards = vec![
            ("shard-001.gguf".into(), None),
            ("shard-002.gguf".into(), None),
        ];
        queue
            .queue_sharded("model/x".into(), "Q4_K_M".into(), shards, 0)
            .unwrap();

        let group_id: ShardGroupId = queue
            .pending
            .front()
            .unwrap()
            .group_id
            .clone()
            .unwrap()
            .into();

        // Two items in group, not last
        assert!(!queue.is_last_in_group(&group_id));

        // Pop one
        queue.pop_next();
        assert!(!queue.is_last_in_group(&group_id));

        // Pop second - now it's "last" (none remaining)
        queue.pop_next();
        assert!(queue.is_last_in_group(&group_id));
    }

    #[test]
    fn test_group_remaining_size() {
        let mut queue = DownloadQueue::new(10);
        let shards = vec![
            ("shard-001.gguf".into(), Some(1000u64)),
            ("shard-002.gguf".into(), Some(2000u64)),
            ("shard-003.gguf".into(), Some(3000u64)),
        ];
        queue
            .queue_sharded("model/x".into(), "Q4_K_M".into(), shards, 0)
            .unwrap();

        let group_id: ShardGroupId = queue
            .pending
            .front()
            .unwrap()
            .group_id
            .clone()
            .unwrap()
            .into();

        assert_eq!(queue.group_remaining_size(&group_id), 6000);

        // Pop first shard
        queue.pop_next();
        assert_eq!(queue.group_remaining_size(&group_id), 5000);

        // Pop second shard
        queue.pop_next();
        assert_eq!(queue.group_remaining_size(&group_id), 3000);
    }

    #[test]
    fn test_group_pending_count() {
        let mut queue = DownloadQueue::new(10);
        let shards = vec![
            ("shard-001.gguf".into(), None),
            ("shard-002.gguf".into(), None),
        ];
        queue
            .queue_sharded("model/x".into(), "Q4_K_M".into(), shards, 0)
            .unwrap();

        let group_id: ShardGroupId = queue
            .pending
            .front()
            .unwrap()
            .group_id
            .clone()
            .unwrap()
            .into();

        assert_eq!(queue.group_pending_count(&group_id), 2);
        queue.pop_next();
        assert_eq!(queue.group_pending_count(&group_id), 1);
    }

    #[test]
    fn test_reorder_single_item() {
        let mut queue = DownloadQueue::new(10);
        queue.queue("model/a".into(), None, 0).unwrap();
        queue.queue("model/b".into(), None, 0).unwrap();
        queue.queue("model/c".into(), None, 0).unwrap();

        // Move model/c to front
        let new_pos = queue.reorder("model/c", 0).unwrap();
        assert_eq!(new_pos, 0);

        let items: Vec<_> = queue.pending.iter().map(|i| i.model_id.as_str()).collect();
        assert_eq!(items, vec!["model/c", "model/a", "model/b"]);
    }

    #[test]
    fn test_reorder_moves_entire_shard_group() {
        let mut queue = DownloadQueue::new(10);
        queue.queue("model/a".into(), None, 0).unwrap();

        let shards = vec![("s1.gguf".into(), None), ("s2.gguf".into(), None)];
        queue
            .queue_sharded("model/b".into(), "Q4".into(), shards, 0)
            .unwrap();

        // Move shard group to front
        queue.reorder("model/b", 0).unwrap();

        let items: Vec<_> = queue.pending.iter().map(|i| i.model_id.as_str()).collect();
        assert_eq!(items, vec!["model/b", "model/b", "model/a"]);

        // Verify shard order preserved
        let shard0 = queue.pending[0].shard_info.as_ref().unwrap();
        let shard1 = queue.pending[1].shard_info.as_ref().unwrap();
        assert_eq!(shard0.shard_index, 0);
        assert_eq!(shard1.shard_index, 1);
    }

    #[test]
    fn test_reorder_not_found() {
        let mut queue = DownloadQueue::new(10);
        let result = queue.reorder("nonexistent", 0);
        assert!(matches!(result, Err(DownloadError::NotInQueue { .. })));
    }

    #[test]
    fn test_status_positions() {
        let mut queue = DownloadQueue::new(10);
        queue.queue("model/a".into(), None, 0).unwrap();
        queue.queue("model/b".into(), None, 0).unwrap();

        let status = queue.status(None);

        // Positions start at 2 (position 1 is reserved for current download)
        assert_eq!(status.pending[0].position, 2);
        assert_eq!(status.pending[0].model_id, "model/a");
        assert_eq!(status.pending[1].position, 3);
        assert_eq!(status.pending[1].model_id, "model/b");
    }

    #[test]
    fn test_status_includes_failed_with_error() {
        let mut queue = DownloadQueue::new(10);
        let item = QueuedDownload::new("model/a".into(), Some("Q4_K_M".into()));
        queue.mark_failed(FailedDownload::new(item, "Connection timeout"));

        let status = queue.status(None);

        assert_eq!(status.failed.len(), 1);
        assert_eq!(status.failed[0].model_id, "model/a");
        assert_eq!(
            status.failed[0].error,
            Some("Connection timeout".to_string())
        );
        assert_eq!(status.failed[0].status, DownloadStatus::Failed);
    }

    #[test]
    fn test_retry_clears_from_failed() {
        let mut queue = DownloadQueue::new(10);

        // First, fail a download
        let item = QueuedDownload::new("model/a".into(), Some("Q4_K_M".into()));
        queue.mark_failed(FailedDownload::new(item, "Network error"));
        assert!(queue.is_failed("model/a"));

        // Re-queue clears from failed
        queue
            .queue("model/a".into(), Some("Q4_K_M".into()), 0)
            .unwrap();

        assert!(!queue.is_failed("model/a"));
        assert!(queue.is_queued("model/a"));
    }

    #[test]
    fn test_get_failed() {
        let mut queue = DownloadQueue::new(10);
        let item = QueuedDownload::new("model/a".into(), Some("Q4_K_M".into()));
        queue.mark_failed(FailedDownload::new(item, "Test error"));

        let failed = queue.get_failed("model/a").unwrap();
        assert_eq!(failed.error, "Test error");
        assert_eq!(failed.item.quantization, Some("Q4_K_M".to_string()));

        assert!(queue.get_failed("nonexistent").is_none());
    }

    #[test]
    fn test_clear_failed() {
        let mut queue = DownloadQueue::new(10);
        let item1 = QueuedDownload::new("model/a".into(), None);
        let item2 = QueuedDownload::new("model/b".into(), None);
        queue.mark_failed(FailedDownload::new(item1, "error1"));
        queue.mark_failed(FailedDownload::new(item2, "error2"));

        assert_eq!(queue.failed_len(), 2);
        queue.clear_failed();
        assert_eq!(queue.failed_len(), 0);
    }

    #[test]
    fn test_shard_group_id_newtype() {
        let id1 = ShardGroupId::new("test-group");
        let id2 = ShardGroupId::from("test-group");
        let id3: ShardGroupId = "test-group".into();

        assert_eq!(id1, id2);
        assert_eq!(id2, id3);
        assert_eq!(id1.as_str(), "test-group");
        assert_eq!(format!("{}", id1), "test-group");
    }

    #[test]
    fn test_shard_group_id_generate() {
        let id1 = ShardGroupId::generate("model/x", "Q4_K_M");
        let id2 = ShardGroupId::generate("model/x", "Q4_K_M");

        // Generated IDs should be unique
        assert_ne!(id1, id2);

        // But contain the model and quantization
        assert!(id1.as_str().contains("model/x"));
        assert!(id1.as_str().contains("Q4_K_M"));
    }
}
