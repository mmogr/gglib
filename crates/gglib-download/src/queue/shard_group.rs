//! Shard group identifier for coordinating multi-file downloads.

use serde::{Deserialize, Serialize};
use std::fmt;

use gglib_core::download::DownloadId;

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
    ///
    /// Uses a simple counter-based approach instead of UUID to avoid
    /// the dependency on uuid crate in this module.
    pub fn generate(download_id: &DownloadId) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("{download_id}:{seq}"))
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
    fn test_shard_group_id_display() {
        let id = ShardGroupId::new("test:group:123");
        assert_eq!(id.to_string(), "test:group:123");
        assert_eq!(id.as_str(), "test:group:123");
    }

    #[test]
    fn test_shard_group_id_generate() {
        let download_id = DownloadId::new("model/test", Some("Q4_K_M"));
        let group1 = ShardGroupId::generate(&download_id);
        let group2 = ShardGroupId::generate(&download_id);

        // Each generated ID should be unique
        assert_ne!(group1, group2);
        // Should contain the download ID as prefix
        assert!(group1.as_str().starts_with("model/test:Q4_K_M:"));
    }

    #[test]
    fn test_shard_group_id_from() {
        let id1 = ShardGroupId::from("test");
        let id2 = ShardGroupId::from(String::from("test"));
        assert_eq!(id1, id2);
    }
}
