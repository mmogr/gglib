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
    /// Size of this shard file in bytes (fetched from HuggingFace API)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
}

impl ShardInfo {
    /// Create a new ShardInfo instance.
    pub fn new(shard_index: usize, total_shards: usize, filename: String) -> Self {
        Self {
            shard_index,
            total_shards,
            filename,
            file_size: None,
        }
    }

    /// Create a new ShardInfo instance with file size.
    pub fn with_size(
        shard_index: usize,
        total_shards: usize,
        filename: String,
        file_size: u64,
    ) -> Self {
        Self {
            shard_index,
            total_shards,
            filename,
            file_size: Some(file_size),
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
    /// Bytes already downloaded (for resumed downloads)
    #[serde(default)]
    pub initial_bytes_downloaded: u64,
    /// Total bytes expected (for resumed downloads)
    #[serde(default)]
    pub initial_total_bytes: u64,
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
            initial_bytes_downloaded: 0,
            initial_total_bytes: 0,
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
            initial_bytes_downloaded: 0,
            initial_total_bytes: 0,
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

    /// Create QueuedDownload items for all shards with file size information.
    ///
    /// Similar to `create_shard_batch` but includes file sizes for aggregate progress tracking.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `quantization` - Quantization type (e.g., "Q4_K_M")
    /// * `shard_files` - Ordered list of (filename, file_size) tuples
    ///
    /// # Returns
    ///
    /// A tuple of (group_id, Vec<QueuedDownload>, total_size)
    pub fn create_shard_batch_with_sizes(
        model_id: &str,
        quantization: &str,
        shard_files: &[(String, u64)],
    ) -> (String, Vec<Self>, u64) {
        let group_id = Self::generate_group_id(model_id, quantization);
        let total_shards = shard_files.len();
        let total_size: u64 = shard_files.iter().map(|(_, size)| size).sum();

        let items: Vec<Self> = shard_files
            .iter()
            .enumerate()
            .map(|(idx, (filename, size))| {
                Self::new_shard(
                    model_id.to_string(),
                    quantization.to_string(),
                    group_id.clone(),
                    ShardInfo::with_size(idx, total_shards, filename.clone(), *size),
                )
            })
            .collect();

        (group_id, items, total_size)
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
    /// Download queue is paused
    Paused,
}

/// State of a paused download for resumption.
/// Stores all information needed to resume the download later.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PausedDownloadState {
    /// The queued download item that was paused
    pub queued_download: QueuedDownload,
    /// Bytes downloaded before pausing (for progress display)
    pub bytes_downloaded: u64,
    /// Total bytes expected (for progress display)
    pub total_bytes: u64,
    /// Timestamp when paused (for display purposes)
    pub paused_at: chrono::DateTime<chrono::Utc>,
}

impl PausedDownloadState {
    /// Create a new paused download state.
    pub fn new(queued_download: QueuedDownload, bytes_downloaded: u64, total_bytes: u64) -> Self {
        Self {
            queued_download,
            bytes_downloaded,
            total_bytes,
            paused_at: chrono::Utc::now(),
        }
    }

    /// Convert paused state back to a QueuedDownload with progress preserved.
    /// This allows the download to resume with correct progress tracking.
    pub fn into_queued_download(self) -> QueuedDownload {
        let mut queued = self.queued_download;
        queued.initial_bytes_downloaded = self.bytes_downloaded;
        queued.initial_total_bytes = self.total_bytes;
        queued
    }
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
    /// Whether the download queue is currently paused
    #[serde(default)]
    pub is_paused: bool,
}

/// Represents an incomplete download that was interrupted (for persistence).
/// Used to restore downloads after app restart.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IncompleteDownload {
    /// Model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    pub model_id: String,
    /// Quantization type if specified
    pub quantization: Option<String>,
    /// Group ID for sharded downloads
    pub group_id: Option<String>,
    /// Shard information if this is part of a sharded model
    pub shard_info: Option<ShardInfo>,
    /// Bytes downloaded before interruption
    pub bytes_downloaded: u64,
    /// Total bytes expected
    pub total_bytes: u64,
    /// Timestamp when the download was interrupted
    pub interrupted_at: String,
}

impl IncompleteDownload {
    /// Create from a paused download state.
    pub fn from_paused(state: &PausedDownloadState) -> Self {
        Self {
            model_id: state.queued_download.model_id.clone(),
            quantization: state.queued_download.quantization.clone(),
            group_id: state.queued_download.group_id.clone(),
            shard_info: state.queued_download.shard_info.clone(),
            bytes_downloaded: state.bytes_downloaded,
            total_bytes: state.total_bytes,
            interrupted_at: state.paused_at.to_rfc3339(),
        }
    }

    /// Convert back to a QueuedDownload for resumption.
    pub fn to_queued_download(&self) -> QueuedDownload {
        QueuedDownload {
            model_id: self.model_id.clone(),
            quantization: self.quantization.clone(),
            group_id: self.group_id.clone(),
            shard_info: self.shard_info.clone(),
            queued_at: Some(std::time::Instant::now()),
            initial_bytes_downloaded: self.bytes_downloaded,
            initial_total_bytes: self.total_bytes,
        }
    }
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
