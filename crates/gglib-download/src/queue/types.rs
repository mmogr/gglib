//! Queue item types (internal implementation).
//!
//! These types are used internally by the queue state machine.
//! For API responses, use the DTO types from `gglib_core::download::queue`.

use std::time::Instant;

use gglib_core::download::{CompletionKey, DownloadId, DownloadStatus, ShardInfo};

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
    /// Stable artifact identity computed at enqueue time.
    /// Used for completion tracking and deduplication.
    pub completion_key: CompletionKey,
}

impl QueuedItem {
    /// Create a new simple (non-sharded) queued download.
    pub fn new(id: DownloadId, completion_key: CompletionKey) -> Self {
        Self {
            id,
            group_id: None,
            shard_info: None,
            revision: None,
            queued_at: Instant::now(),
            completion_key,
        }
    }

    /// Create a new sharded download item.
    pub fn new_shard(
        id: DownloadId,
        group_id: ShardGroupId,
        shard_info: ShardInfo,
        completion_key: CompletionKey,
    ) -> Self {
        Self {
            id,
            group_id: Some(group_id),
            shard_info: Some(shard_info),
            revision: None,
            queued_at: Instant::now(),
            completion_key,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::download::CompletionKey;

    fn test_completion_key(id: &DownloadId) -> CompletionKey {
        CompletionKey::HfFile {
            repo_id: id.model_id().to_string(),
            revision: "test-revision".to_string(),
            filename_canon: "test-model.gguf".to_string(),
            quantization: id.quantization().map(ToString::to_string),
        }
    }

    #[test]
    fn test_queued_item_creation() {
        let id = DownloadId::new("model/test", Some("Q4_K_M"));
        let key = test_completion_key(&id);
        let item = QueuedItem::new(id.clone(), key);

        assert_eq!(item.id, id);
        assert!(item.shard_info.is_none());
        assert!(item.group_id.is_none());
    }

    #[test]
    fn test_queued_item_shard() {
        let id = DownloadId::new("model/test", Some("Q4_K_M"));
        let group_id = ShardGroupId::new("test-group");
        let shard_info = ShardInfo::new(0, 2, "shard-00001.gguf".to_string());
        let key = test_completion_key(&id);

        let item = QueuedItem::new_shard(id, group_id.clone(), shard_info, key);

        assert!(item.shard_info.is_some());
        assert_eq!(item.group_id.as_ref(), Some(&group_id));
    }

    #[test]
    fn test_failed_item() {
        let id = DownloadId::new("model/test", Some("Q4_K_M"));
        let key = test_completion_key(&id);
        let item = QueuedItem::new(id, key);
        let failed = FailedItem::new(item, "Network timeout");

        assert_eq!(failed.error, "Network timeout");
    }

    #[test]
    fn test_failed_item_from_queued_item() {
        let id = DownloadId::new("author/model-name", Some("Q8_0"));
        let key = test_completion_key(&id);
        let item = QueuedItem::new(id.clone(), key);
        let error_msg = "Connection refused";
        let failed = FailedItem::new(item, error_msg);

        // Error message is captured
        assert_eq!(failed.error, error_msg);

        // Original queued item is preserved
        assert_eq!(failed.item.id, id);

        // DTO conversion propagates model info and error
        let dto = failed.to_dto();
        assert_eq!(dto.id, "author/model-name:Q8_0");
        assert_eq!(dto.error, error_msg);
        assert_eq!(dto.failed_at, 0);
    }

    #[test]
    fn test_queued_item_to_dto_with_all_statuses() {
        let id = DownloadId::new("model/test", Some("Q4_K_M"));
        let key = test_completion_key(&id);
        let item = QueuedItem::new(id.clone(), key);

        let statuses = vec![
            DownloadStatus::Queued,
            DownloadStatus::Downloading,
            DownloadStatus::Finalizing,
            DownloadStatus::Registering,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
            DownloadStatus::Cancelled,
        ];

        for status in statuses {
            let dto = item.to_dto(1, status);

            // Status should match what was passed
            assert_eq!(dto.status, status, "Status mismatch for {:?}", status);

            // Position should be preserved
            assert_eq!(dto.position, 1);

            // ID fields should be populated
            assert_eq!(dto.id, "model/test:Q4_K_M");

            // Model ID should match
            assert_eq!(dto.model_id.as_str(), "model/test");
        }
    }
}
