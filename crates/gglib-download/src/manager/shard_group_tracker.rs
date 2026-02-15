//! Shard group tracker for coordinating multi-shard downloads.
//!
//! This module provides a pure state tracker that accumulates shard completion
//! events and signals when all shards in a group have been downloaded.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

#[cfg(test)]
use std::time::Duration;

use gglib_core::download::Quantization;
use gglib_core::ports::ResolvedFile;

use crate::queue::ShardGroupId;

/// Metadata needed to register a completed model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMetadata {
    /// Repository ID (e.g., "unsloth/Llama-3-GGUF").
    pub repo_id: String,
    /// Commit SHA at time of download.
    pub commit_sha: String,
    /// The resolved quantization.
    pub quantization: Quantization,
    /// Primary filename (first shard filename).
    pub primary_filename: String,
    /// `HuggingFace` tags for the model.
    pub hf_tags: Vec<String>,
    /// File entries with OIDs from resolution (for `model_files` table).
    pub file_entries: Vec<ResolvedFile>,
}

/// State for a shard group being tracked.
#[derive(Debug)]
struct ShardGroupState {
    /// Downloaded paths indexed by shard number.
    paths_by_index: Vec<Option<PathBuf>>,
    /// Total number of shards expected.
    expected_total: u32,
    /// Metadata for model registration.
    metadata: GroupMetadata,
    /// Last time this group was updated.
    last_updated: Instant,
}

impl ShardGroupState {
    /// Create a new shard group state.
    fn new(expected_total: u32, metadata: GroupMetadata) -> Self {
        Self {
            paths_by_index: vec![None; expected_total as usize],
            expected_total,
            metadata,
            last_updated: Instant::now(),
        }
    }

    /// Record a shard completion (idempotent per index).
    ///
    /// If a path is already recorded for this index, it is kept (first-wins).
    fn record_shard(&mut self, index: u32, path: PathBuf) {
        if (index as usize) < self.paths_by_index.len() {
            let slot = &mut self.paths_by_index[index as usize];
            if slot.is_none() {
                *slot = Some(path);
            }
            self.last_updated = Instant::now();
        }
    }

    /// Check if all shards have been downloaded.
    fn is_complete(&self) -> bool {
        self.paths_by_index.len() == self.expected_total as usize
            && self.paths_by_index.iter().all(Option::is_some)
    }

    /// Extract ordered paths (only call if `is_complete`).
    fn ordered_paths(&self) -> Vec<PathBuf> {
        self.paths_by_index
            .iter()
            .filter_map(Clone::clone)
            .collect()
    }
}

/// Complete shard group ready for registration.
#[derive(Debug)]
pub struct GroupComplete {
    /// All shard paths in order.
    pub ordered_paths: Vec<PathBuf>,
    /// Metadata for model registration.
    pub metadata: GroupMetadata,
}

/// Tracker for coordinating shard group completion.
///
/// This is a pure state machine that accumulates shard completions
/// and signals when groups are complete. No I/O or locking happens here.
#[derive(Debug, Default)]
pub struct ShardGroupTracker {
    /// Active shard groups being tracked.
    ///
    /// INVARIANT: `groups` contains ONLY in-progress groups.
    /// Terminal paths (completion, failure, cancel) MUST remove entries from `groups`.
    groups: HashMap<ShardGroupId, ShardGroupState>,
}

impl ShardGroupTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a shard completion.
    ///
    /// Returns `Some(GroupComplete)` if this was the last shard needed.
    /// This method is idempotent - recording the same shard index twice
    /// will not cause issues.
    ///
    /// # Arguments
    ///
    /// * `group_id` - The shard group identifier
    /// * `index` - Zero-based shard index
    /// * `path` - Path to the downloaded shard file
    /// * `expected_total` - Total number of shards in the group
    /// * `metadata` - Metadata for the model (used on first call)
    ///
    /// # Panics (debug builds only)
    ///
    /// In debug builds, panics if metadata for this group doesn't match previously recorded
    /// metadata. This catches bugs where shards compute different identities.
    pub fn on_shard_done(
        &mut self,
        group_id: &ShardGroupId,
        index: u32,
        path: PathBuf,
        expected_total: u32,
        metadata: &GroupMetadata,
    ) -> Option<GroupComplete> {
        // Get or create the group state
        let state = self
            .groups
            .entry(group_id.clone())
            .or_insert_with(|| ShardGroupState::new(expected_total, metadata.clone()));

        // Guard: in debug builds, assert metadata consistency
        debug_assert_eq!(
            state.metadata, *metadata,
            "Metadata mismatch for group {group_id:?}! All shards must compute identical identity."
        );

        // Record this shard (idempotent)
        state.record_shard(index, path);

        // Check if complete
        if state.is_complete() {
            // Remove from tracking and return complete group
            if let Some(state) = self.groups.remove(group_id) {
                return Some(GroupComplete {
                    ordered_paths: state.ordered_paths(),
                    metadata: state.metadata,
                });
            }
        }

        None
    }

    /// Remove a shard group that was cancelled or failed.
    ///
    /// This prevents memory leaks from incomplete downloads.
    pub fn on_group_failed(&mut self, group_id: &ShardGroupId) {
        self.groups.remove(group_id);
    }

    /// Check if there are any in-progress shard groups.
    ///
    /// Returns `true` if any shard groups are still incomplete (waiting for shards).
    /// This is used for drain detection: the queue is only truly drained if both
    /// the pending queue is empty AND `has_open_groups()` returns `false`.
    ///
    /// INVARIANT: This relies on terminal paths removing groups from `self.groups`.
    pub fn has_open_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Get the number of active shard groups.
    #[cfg(test)]
    pub fn active_count(&self) -> usize {
        self.groups.len()
    }

    /// Remove expired shard groups (for testing).
    #[cfg(test)]
    pub fn gc_expired(&mut self, ttl: Duration) -> usize {
        let now = Instant::now();
        let before_count = self.groups.len();

        self.groups
            .retain(|_, state| now.duration_since(state.last_updated) < ttl);

        before_count - self.groups.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> GroupMetadata {
        GroupMetadata {
            repo_id: "test/model".to_string(),
            commit_sha: "abc123".to_string(),
            quantization: Quantization::Q4KM,
            primary_filename: "model-00001-of-00003.gguf".to_string(),
            hf_tags: vec![],
            file_entries: vec![],
        }
    }

    #[test]
    fn test_out_of_order_completion() {
        let mut tracker = ShardGroupTracker::new();
        let group_id = ShardGroupId::new("test-group");
        let metadata = test_metadata();

        // Complete shards out of order: 2, 0, 1
        let result1 = tracker.on_shard_done(
            &group_id,
            2,
            PathBuf::from("/path/shard-2.gguf"),
            3,
            &metadata,
        );
        assert!(result1.is_none(), "Should not complete after shard 2");

        let result2 = tracker.on_shard_done(
            &group_id,
            0,
            PathBuf::from("/path/shard-0.gguf"),
            3,
            &metadata,
        );
        assert!(result2.is_none(), "Should not complete after shard 0");

        let result3 = tracker.on_shard_done(
            &group_id,
            1,
            PathBuf::from("/path/shard-1.gguf"),
            3,
            &metadata,
        );
        assert!(result3.is_some(), "Should complete after shard 1");

        let complete = result3.unwrap();
        assert_eq!(complete.ordered_paths.len(), 3);
        assert_eq!(
            complete.ordered_paths[0],
            PathBuf::from("/path/shard-0.gguf")
        );
        assert_eq!(
            complete.ordered_paths[1],
            PathBuf::from("/path/shard-1.gguf")
        );
        assert_eq!(
            complete.ordered_paths[2],
            PathBuf::from("/path/shard-2.gguf")
        );
    }

    #[test]
    fn test_idempotent_shard_recording() {
        let mut tracker = ShardGroupTracker::new();
        let group_id = ShardGroupId::new("test-group");
        let metadata = test_metadata();

        // Record shard 0 twice
        tracker.on_shard_done(
            &group_id,
            0,
            PathBuf::from("/path/shard-0.gguf"),
            2,
            &metadata,
        );
        tracker.on_shard_done(
            &group_id,
            0,
            PathBuf::from("/path/shard-0-duplicate.gguf"),
            2,
            &metadata,
        );

        // Complete with shard 1
        let result = tracker.on_shard_done(
            &group_id,
            1,
            PathBuf::from("/path/shard-1.gguf"),
            2,
            &metadata,
        );

        assert!(result.is_some());
        let complete = result.unwrap();
        assert_eq!(complete.ordered_paths.len(), 2);
        // First recording of shard 0 should be kept
        assert_eq!(
            complete.ordered_paths[0],
            PathBuf::from("/path/shard-0.gguf")
        );
    }

    #[test]
    fn test_on_group_failed_cleanup() {
        let mut tracker = ShardGroupTracker::new();
        let group_id = ShardGroupId::new("test-group");
        let metadata = test_metadata();

        // Start a group
        tracker.on_shard_done(
            &group_id,
            0,
            PathBuf::from("/path/shard-0.gguf"),
            3,
            &metadata,
        );

        assert_eq!(tracker.active_count(), 1);

        // Mark as failed
        tracker.on_group_failed(&group_id);

        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_gc_expired() {
        let mut tracker = ShardGroupTracker::new();
        let group_id = ShardGroupId::new("test-group");
        let metadata = test_metadata();

        // Add a shard
        tracker.on_shard_done(
            &group_id,
            0,
            PathBuf::from("/path/shard-0.gguf"),
            3,
            &metadata,
        );

        assert_eq!(tracker.active_count(), 1);

        // GC with large TTL - should not remove
        let removed = tracker.gc_expired(Duration::from_secs(3600));
        assert_eq!(removed, 0);
        assert_eq!(tracker.active_count(), 1);

        // GC with zero TTL - should remove
        let removed = tracker.gc_expired(Duration::from_secs(0));
        assert_eq!(removed, 1);
        assert_eq!(tracker.active_count(), 0);
    }

    // =========================================================================
    // Terminal Path Invariant Tests
    // =========================================================================

    #[test]
    fn test_invariant_completion_removes_group() {
        let mut tracker = ShardGroupTracker::new();
        let group_id = ShardGroupId::new("complete-group");
        let metadata = test_metadata();

        // Start tracking a 2-shard group
        tracker.on_shard_done(&group_id, 0, PathBuf::from("/s0"), 2, &metadata);
        assert!(
            tracker.has_open_groups(),
            "Group should be open after first shard"
        );
        assert_eq!(tracker.active_count(), 1);

        // Complete the group
        let result = tracker.on_shard_done(&group_id, 1, PathBuf::from("/s1"), 2, &metadata);
        assert!(result.is_some(), "Should return GroupComplete");

        // INVARIANT: completion must remove the group
        assert!(
            !tracker.has_open_groups(),
            "Completion must remove group from tracker"
        );
        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_invariant_failure_removes_group() {
        let mut tracker = ShardGroupTracker::new();
        let group_id = ShardGroupId::new("failed-group");
        let metadata = test_metadata();

        // Start tracking a group
        tracker.on_shard_done(&group_id, 0, PathBuf::from("/s0"), 3, &metadata);
        assert!(tracker.has_open_groups(), "Group should be open");
        assert_eq!(tracker.active_count(), 1);

        // Mark group as failed
        tracker.on_group_failed(&group_id);

        // INVARIANT: failure must remove the group
        assert!(
            !tracker.has_open_groups(),
            "Failure must remove group from tracker"
        );
        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_invariant_multiple_groups_drain_correctly() {
        let mut tracker = ShardGroupTracker::new();
        let group_a = ShardGroupId::new("group-a");
        let group_b = ShardGroupId::new("group-b");
        let metadata = test_metadata();

        // Start two groups
        tracker.on_shard_done(&group_a, 0, PathBuf::from("/a0"), 2, &metadata);
        tracker.on_shard_done(&group_b, 0, PathBuf::from("/b0"), 2, &metadata);
        assert_eq!(tracker.active_count(), 2);

        // Complete group A
        tracker.on_shard_done(&group_a, 1, PathBuf::from("/a1"), 2, &metadata);
        assert_eq!(tracker.active_count(), 1, "Group A should be removed");
        assert!(tracker.has_open_groups(), "Group B still in progress");

        // Fail group B
        tracker.on_group_failed(&group_b);
        assert_eq!(tracker.active_count(), 0, "Group B should be removed");
        assert!(!tracker.has_open_groups(), "No groups should remain");
    }

    #[test]
    fn test_has_open_groups_empty_tracker() {
        let tracker = ShardGroupTracker::new();
        assert!(
            !tracker.has_open_groups(),
            "Empty tracker has no open groups"
        );
        assert_eq!(tracker.active_count(), 0);
    }
}
