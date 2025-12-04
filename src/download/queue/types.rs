//! Queue item types.

use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::download::domain::events::{DownloadStatus, DownloadSummary};
use crate::download::domain::types::{DownloadId, ShardInfo};
use crate::download::queue::ShardGroupId;

/// A queued download item waiting to be processed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedDownload {
    /// The download identifier.
    pub id: DownloadId,
    /// Links shards of the same model together for group operations.
    pub group_id: Option<ShardGroupId>,
    /// Shard-specific information if this is part of a sharded model.
    pub shard_info: Option<ShardInfo>,
    /// When this item was queued (for ordering/debugging).
    #[serde(skip)]
    pub queued_at: Option<Instant>,
}

impl QueuedDownload {
    /// Create a new simple (non-sharded) queued download.
    pub fn new(id: DownloadId) -> Self {
        Self {
            id,
            group_id: None,
            shard_info: None,
            queued_at: Some(Instant::now()),
        }
    }

    /// Create a new sharded download item.
    pub fn new_shard(id: DownloadId, group_id: ShardGroupId, shard_info: ShardInfo) -> Self {
        Self {
            id,
            group_id: Some(group_id),
            shard_info: Some(shard_info),
            queued_at: Some(Instant::now()),
        }
    }

    /// Check if this item is part of a shard group.
    pub fn is_shard(&self) -> bool {
        self.shard_info.is_some()
    }

    /// Get the canonical ID string.
    pub fn canonical_id(&self) -> String {
        self.id.to_string()
    }

    /// Convert to a DownloadSummary for API responses.
    pub fn to_summary(&self, position: u32, status: DownloadStatus) -> DownloadSummary {
        let display_name = if let Some(ref shard) = self.shard_info {
            format!("{} ({})", self.id, shard.display())
        } else {
            self.id.to_string()
        };

        DownloadSummary {
            id: self.canonical_id(),
            display_name,
            status,
            position,
            error: None,
            group_id: self.group_id.as_ref().map(|g| g.to_string()),
            shard_info: self.shard_info.clone(),
        }
    }
}

/// A failed download with error information.
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

    /// Convert to a DownloadSummary for API responses.
    pub fn to_summary(&self, position: u32) -> DownloadSummary {
        let mut summary = self.item.to_summary(position, DownloadStatus::Failed);
        summary.error = Some(self.error.clone());
        summary
    }
}

/// Complete snapshot of the queue state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueSnapshot {
    /// Currently downloading item (if any).
    pub current: Option<DownloadSummary>,
    /// Items waiting in the queue.
    pub pending: Vec<DownloadSummary>,
    /// Recently failed downloads (for retry).
    pub failed: Vec<DownloadSummary>,
    /// Maximum queue capacity.
    pub max_size: u32,
}

impl QueueSnapshot {
    /// Get all items as a flat list (for QueueSnapshot event).
    pub fn all_items(&self) -> Vec<DownloadSummary> {
        let mut items = Vec::new();
        if let Some(ref current) = self.current {
            items.push(current.clone());
        }
        items.extend(self.pending.clone());
        items.extend(self.failed.clone());
        items
    }

    /// Get the total number of items (current + pending).
    pub fn queue_length(&self) -> usize {
        let current_count = if self.current.is_some() { 1 } else { 0 };
        current_count + self.pending.len()
    }
}
