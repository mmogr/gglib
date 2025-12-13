//! Queue item types (internal implementation).
//!
//! These types are used internally by the queue state machine.
//! For API responses, use the DTO types from `gglib_core::download::queue`.

// TODO(#221): Remove after Phase 2.5 completes
#![allow(unused_imports)]

use std::time::Instant;

use serde::{Deserialize, Serialize};

use gglib_core::download::{DownloadId, DownloadStatus, ShardInfo};

use super::shard_group::ShardGroupId;

/// A queued download item waiting to be processed.
///
/// This is an internal type for the queue state machine.
/// For serialization to APIs, convert to `gglib_core::download::QueuedDownload`.
#[derive(Clone, Debug)]
pub struct QueuedItem {
    /// The download identifier.
    pub id: DownloadId,
    /// Links shards of the same model together for group operations.
    pub group_id: Option<ShardGroupId>,
    /// Shard-specific information if this is part of a sharded model.
    pub shard_info: Option<ShardInfo>,
    /// Git revision/tag/commit (e.g., "main", "v1.0", SHA).
    pub revision: Option<String>,
    /// When this item was queued (for ordering/debugging).
    pub queued_at: Instant,
}

impl QueuedItem {
    /// Create a new simple (non-sharded) queued download.
    pub fn new(id: DownloadId) -> Self {
        Self {
            id,
            group_id: None,
            shard_info: None,
            revision: None,
            queued_at: Instant::now(),
        }
    }

    /// Create a new sharded download item.
    pub fn new_shard(id: DownloadId, group_id: ShardGroupId, shard_info: ShardInfo) -> Self {
        Self {
            id,
            group_id: Some(group_id),
            shard_info: Some(shard_info),
            revision: None,
            queued_at: Instant::now(),
        }
    }

    /// Check if this item is part of a shard group.
    pub const fn is_shard(&self) -> bool {
        self.shard_info.is_some()
    }

    /// Get the canonical ID string.
    pub fn canonical_id(&self) -> String {
        self.id.to_string()
    }

    /// Convert to a core DTO for API responses.
    pub fn to_dto(
        &self,
        position: u32,
        status: DownloadStatus,
    ) -> gglib_core::download::QueuedDownload {
        let display_name = self.shard_info.as_ref().map_or_else(
            || self.id.to_string(),
            |shard| format!("{} ({})", self.id, shard.display()),
        );

        let mut dto = gglib_core::download::QueuedDownload::new(
            self.canonical_id(),
            self.id.model_id(),
            display_name,
            position,
            self.queued_at.elapsed().as_secs(), // Approximate queued_at as epoch
        );
        dto.status = status;
        dto.group_id = self.group_id.as_ref().map(std::string::ToString::to_string);
        dto.shard_info.clone_from(&self.shard_info);

        dto
    }
}

/// A failed download with error information.
#[derive(Clone, Debug)]
pub struct FailedItem {
    /// The original queued download item.
    pub item: QueuedItem,
    /// Human-readable error message.
    pub error: String,
    /// When the failure occurred.
    pub failed_at: Instant,
}

impl FailedItem {
    /// Create a new failed download entry.
    pub fn new(item: QueuedItem, error: impl Into<String>) -> Self {
        Self {
            item,
            error: error.into(),
            failed_at: Instant::now(),
        }
    }

    /// Convert to a core DTO for API responses.
    pub fn to_dto(&self) -> gglib_core::download::FailedDownload {
        let display_name = self.item.shard_info.as_ref().map_or_else(
            || self.item.id.to_string(),
            |shard| format!("{} ({})", self.item.id, shard.display()),
        );

        gglib_core::download::FailedDownload::new(
            self.item.canonical_id(),
            display_name,
            &self.error,
            self.failed_at.elapsed().as_secs(),
        )
    }
}

/// Commands that can be applied to the queue.
///
/// Using an explicit command type makes the state machine easier to test
/// and reason about.
#[derive(Debug, Clone)]
pub enum QueueCommand {
    /// Add a single download to the queue.
    Enqueue { id: DownloadId },
    /// Add a sharded download (multiple files).
    EnqueueSharded {
        id: DownloadId,
        shard_files: Vec<(String, Option<u64>)>,
    },
    /// Remove a download from the queue.
    Remove { id: DownloadId },
    /// Move a download to a new position.
    Reorder { id: DownloadId, new_position: u32 },
    /// Mark a download as failed.
    MarkFailed { id: DownloadId, error: String },
    /// Retry a failed download.
    RetryFailed { id: DownloadId },
    /// Clear all items.
    Clear,
}

/// Events emitted by the queue in response to commands.
///
/// These events can be used to trigger side effects (persistence, UI updates, etc.)
#[derive(Debug, Clone)]
pub enum QueueEvent {
    /// A download was added to the queue.
    Enqueued { id: DownloadId, position: u32 },
    /// A download was removed from the queue.
    Removed { id: DownloadId },
    /// A download was reordered.
    Reordered { id: DownloadId, new_position: u32 },
    /// A download failed.
    Failed { id: DownloadId, error: String },
    /// A failed download is being retried.
    Retrying { id: DownloadId, position: u32 },
    /// The queue was cleared.
    Cleared,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queued_item_creation() {
        let id = DownloadId::new("model/test", Some("Q4_K_M"));
        let item = QueuedItem::new(id.clone());

        assert_eq!(item.id, id);
        assert!(!item.is_shard());
        assert!(item.group_id.is_none());
    }

    #[test]
    fn test_queued_item_shard() {
        let id = DownloadId::new("model/test", Some("Q4_K_M"));
        let group_id = ShardGroupId::new("test-group");
        let shard_info = ShardInfo::new(0, 2, "shard-00001.gguf".to_string());

        let item = QueuedItem::new_shard(id, group_id.clone(), shard_info);

        assert!(item.is_shard());
        assert_eq!(item.group_id.as_ref(), Some(&group_id));
    }

    #[test]
    fn test_failed_item() {
        let id = DownloadId::new("model/test", Some("Q4_K_M"));
        let item = QueuedItem::new(id);
        let failed = FailedItem::new(item, "Network timeout");

        assert_eq!(failed.error, "Network timeout");
    }
}
