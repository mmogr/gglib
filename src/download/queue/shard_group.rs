//! Shard group identifier for coordinating multi-file downloads.

use crate::download::domain::types::DownloadId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a shard group.
///
/// Groups multiple shard downloads together for coordinated operations
/// (cancel all, fail all, retry all). The group ID contains the download ID
/// plus a unique suffix to distinguish different download attempts.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShardGroupId(String);

impl ShardGroupId {
    /// Create a new shard group ID from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a unique group ID for a download.
    pub fn generate(download_id: &DownloadId) -> Self {
        Self(format!("{}:{}", download_id, uuid::Uuid::new_v4()))
    }

    /// Get the inner string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ShardGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

impl AsRef<str> for ShardGroupId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_group_id_new() {
        let id = ShardGroupId::new("test-group");
        assert_eq!(id.as_str(), "test-group");
        assert_eq!(format!("{}", id), "test-group");
    }

    #[test]
    fn test_shard_group_id_generate() {
        let download_id = DownloadId::new("model/x", Some("Q4_K_M"));
        let id1 = ShardGroupId::generate(&download_id);
        let id2 = ShardGroupId::generate(&download_id);

        // Generated IDs should be unique
        assert_ne!(id1, id2);

        // But contain the download ID
        assert!(id1.as_str().contains("model/x"));
        assert!(id1.as_str().contains("Q4_K_M"));
    }

    #[test]
    fn test_shard_group_id_from() {
        let id1: ShardGroupId = "test".into();
        let id2 = ShardGroupId::from("test");
        let id3 = ShardGroupId::from("test".to_string());

        assert_eq!(id1, id2);
        assert_eq!(id2, id3);
    }

    #[test]
    fn test_shard_group_id_equality() {
        let id1 = ShardGroupId::new("group-123");
        let id2 = ShardGroupId::new("group-123");
        let id3 = ShardGroupId::new("group-456");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }
}
