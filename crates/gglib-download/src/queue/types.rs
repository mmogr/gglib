//! Queue item types (internal implementation).
//!
//! These types are used internally by the queue state machine.
//! For API responses, use the DTO types from `gglib_core::download::queue`.

use gglib_core::download::{CompletionKey, DownloadId, ShardInfo};

use super::shard_group::ShardGroupId;

/// A queued download item waiting to be processed.
///
/// This is an internal type for the queue state machine.
/// What a client is shown of it is a row, built in `rows`.
#[derive(Clone, Debug)]
pub(crate) struct QueuedItem {
    /// The download identifier.
    pub id: DownloadId,
    /// Links shards of the same model together for group operations.
    pub group_id: Option<ShardGroupId>,
    /// Shard-specific information if this is part of a sharded model.
    pub shard_info: Option<ShardInfo>,
    /// Git revision/tag/commit (e.g., "main", "v1.0", SHA).
    pub revision: Option<String>,
    /// Stable artifact identity computed at enqueue time.
    /// Used for completion tracking and deduplication.
    pub completion_key: CompletionKey,
}

impl QueuedItem {
    /// Create a new simple (non-sharded) queued download.
    #[cfg(test)]
    pub(crate) fn new(id: DownloadId, completion_key: CompletionKey) -> Self {
        Self {
            id,
            group_id: None,
            shard_info: None,
            revision: None,
            completion_key,
        }
    }

    /// Create a new sharded download item.
    pub(crate) fn new_shard(
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
            completion_key,
        }
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
}
