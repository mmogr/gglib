//! Data models for the download service.
//!
//! This module contains all the data structures used by the download queue system,
//! including queue items, status types, and shard information.
//!
//! Types are organized here separately from `download_service.rs` to keep the
//! service file focused on business logic, following the pattern used by
//! `proxy/models.rs` and `database/models.rs`.

use serde::{Deserialize, Serialize};
use std::time::Instant;
use thiserror::Error;

/// Errors related to download operations.
#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("Download '{model_id}' was cancelled by the user")]
    Cancelled { model_id: String },

    #[error("A download for '{model_id}' is already running or queued")]
    AlreadyRunning { model_id: String },

    #[error("No active download for '{model_id}'")]
    NotFound { model_id: String },

    #[error("Download queue is full (max {max_size} items)")]
    QueueFull { max_size: u32 },

    #[error("Item '{model_id}' not found in queue")]
    NotInQueue { model_id: String },

    #[error("Shard group '{group_id}' not found")]
    GroupNotFound { group_id: String },
}

/// Information about a shard within a sharded model download.
///
/// Used to track individual parts of a multi-file GGUF model.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ShardInfo {
    /// 0-based index of this shard (e.g., 0 for "Part 1/3")
    pub shard_index: usize,
    /// Total number of shards in this model
    pub total_shards: usize,
    /// The specific filename for this shard (e.g., "model-00001-of-00003.gguf")
    pub filename: String,
}

impl ShardInfo {
    /// Create a new ShardInfo instance.
    pub fn new(shard_index: usize, total_shards: usize, filename: String) -> Self {
        Self {
            shard_index,
            total_shards,
            filename,
        }
    }

    /// Format as display string (e.g., "Part 1/3")
    pub fn display(&self) -> String {
        format!("Part {}/{}", self.shard_index + 1, self.total_shards)
    }
}

/// A queued download item waiting to be processed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedDownload {
    pub model_id: String,
    pub quantization: Option<String>,
    /// Links shards of the same model together for group operations
    pub group_id: Option<String>,
    /// Shard-specific information if this is part of a sharded model
    pub shard_info: Option<ShardInfo>,
    #[serde(skip)]
    pub queued_at: Option<Instant>,
}

impl QueuedDownload {
    /// Create a new simple (non-sharded) queued download.
    pub fn new(model_id: String, quantization: Option<String>) -> Self {
        Self {
            model_id,
            quantization,
            group_id: None,
            shard_info: None,
            queued_at: Some(Instant::now()),
        }
    }

    /// Create a new sharded download item.
    pub fn new_shard(
        model_id: String,
        quantization: String,
        group_id: String,
        shard_info: ShardInfo,
    ) -> Self {
        Self {
            model_id,
            quantization: Some(quantization),
            group_id: Some(group_id),
            shard_info: Some(shard_info),
            queued_at: Some(Instant::now()),
        }
    }

    /// Check if this item is part of a shard group.
    pub fn is_shard(&self) -> bool {
        self.shard_info.is_some()
    }

    /// Generate a unique group ID for a sharded download.
    pub fn generate_group_id(model_id: &str, quantization: &str) -> String {
        format!("{}:{}:{}", model_id, quantization, uuid::Uuid::new_v4())
    }

    /// Create QueuedDownload items for all shards of a model.
    ///
    /// Each shard is created with a shared `group_id` for group operations
    /// (cancel all, fail all, retry all).
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `quantization` - Quantization type (e.g., "Q4_K_M")
    /// * `shard_filenames` - Ordered list of shard filenames
    ///
    /// # Returns
    ///
    /// A tuple of (group_id, Vec<QueuedDownload>)
    pub fn create_shard_batch(
        model_id: &str,
        quantization: &str,
        shard_filenames: &[String],
    ) -> (String, Vec<Self>) {
        let group_id = Self::generate_group_id(model_id, quantization);
        let total_shards = shard_filenames.len();

        let items: Vec<Self> = shard_filenames
            .iter()
            .enumerate()
            .map(|(idx, filename)| {
                Self::new_shard(
                    model_id.to_string(),
                    quantization.to_string(),
                    group_id.clone(),
                    ShardInfo::new(idx, total_shards, filename.clone()),
                )
            })
            .collect();

        (group_id, items)
    }
}

/// Status of a download in the queue.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    /// Currently being downloaded
    Downloading,
    /// Waiting in queue
    Queued,
    /// Completed successfully
    Completed,
    /// Failed with an error
    Failed,
}

/// Information about a download in the queue (for API responses).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadQueueItem {
    pub model_id: String,
    pub quantization: Option<String>,
    pub status: DownloadStatus,
    /// Position in queue (1 = currently downloading)
    pub position: usize,
    /// Error message if status is Failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Links shards of the same model together for group operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Shard-specific information if this is part of a sharded model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard_info: Option<ShardInfo>,
}

impl DownloadQueueItem {
    /// Create from a QueuedDownload with position and status.
    pub fn from_queued(item: &QueuedDownload, position: usize, status: DownloadStatus) -> Self {
        Self {
            model_id: item.model_id.clone(),
            quantization: item.quantization.clone(),
            status,
            position,
            error: None,
            group_id: item.group_id.clone(),
            shard_info: item.shard_info.clone(),
        }
    }

    /// Create a minimal item for active downloads (where we only know model_id).
    pub fn active(model_id: String) -> Self {
        Self {
            model_id,
            quantization: None,
            status: DownloadStatus::Downloading,
            position: 1,
            error: None,
            group_id: None,
            shard_info: None,
        }
    }
}

/// Complete queue status including current download and pending items.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadQueueStatus {
    /// Currently downloading item (if any)
    pub current: Option<DownloadQueueItem>,
    /// Items waiting in the queue
    pub pending: Vec<DownloadQueueItem>,
    /// Recently failed downloads (for retry)
    pub failed: Vec<DownloadQueueItem>,
    /// Maximum queue size
    pub max_size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_info_display() {
        // shard_index is 0-based, but display shows 1-based for humans
        let shard = ShardInfo::new(1, 5, "model-00002-of-00005.gguf".to_string());
        assert_eq!(shard.display(), "Part 2/5");
    }

    #[test]
    fn test_queued_download_is_shard() {
        let simple = QueuedDownload::new("test/model".to_string(), None);
        assert!(!simple.is_shard());

        let shard = QueuedDownload::new_shard(
            "test/model".to_string(),
            "Q4_K_M".to_string(),
            "group-123".to_string(),
            ShardInfo::new(0, 3, "file.gguf".to_string()),
        );
        assert!(shard.is_shard());
    }

    #[test]
    fn test_create_shard_batch() {
        let filenames = vec![
            "model-00001-of-00003.gguf".to_string(),
            "model-00002-of-00003.gguf".to_string(),
            "model-00003-of-00003.gguf".to_string(),
        ];

        let (group_id, items) =
            QueuedDownload::create_shard_batch("test/model", "Q4_K_M", &filenames);

        assert!(!group_id.is_empty());
        assert!(group_id.contains("test/model"));
        assert!(group_id.contains("Q4_K_M"));
        assert_eq!(items.len(), 3);

        // Check first shard (0-based index)
        assert_eq!(items[0].model_id, "test/model");
        assert_eq!(items[0].quantization, Some("Q4_K_M".to_string()));
        assert_eq!(items[0].group_id, Some(group_id.clone()));
        let shard0 = items[0].shard_info.as_ref().unwrap();
        assert_eq!(shard0.shard_index, 0);
        assert_eq!(shard0.total_shards, 3);
        assert_eq!(shard0.filename, "model-00001-of-00003.gguf");

        // Check last shard (0-based index)
        let shard2 = items[2].shard_info.as_ref().unwrap();
        assert_eq!(shard2.shard_index, 2);
        assert_eq!(shard2.total_shards, 3);
    }
}
