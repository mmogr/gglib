//! Shard group tracker for coordinating multi-shard downloads.
//!
//! This module provides a pure state tracker that accumulates shard completion
//! events and signals when all shards in a group have been downloaded.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

use gglib_core::download::Quantization;
use gglib_core::ports::ResolvedFile;

use crate::queue::ShardGroupId;

/// Metadata needed to register a completed model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupMetadata {
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
pub(crate) struct GroupComplete {
    /// All shard paths in order.
    pub ordered_paths: Vec<PathBuf>,
    /// Metadata for model registration.
    pub metadata: GroupMetadata,
}

/// How many closed groups the tracker remembers. The manager closes a group
/// as its download ends, when no file of it is being fetched or waiting, so
/// it sends none after; the list is a second guard, and this many is margin.
const CLOSED_LIMIT: usize = 32;

/// Tracker for coordinating shard group completion.
///
/// This is a pure state machine that accumulates shard completions
/// and signals when groups are complete. No I/O or locking happens here.
#[derive(Debug, Default)]
pub(crate) struct ShardGroupTracker {
    /// Active shard groups being tracked.
    ///
    /// INVARIANT: `groups` contains ONLY in-progress groups.
    /// Terminal paths (completion, failure, cancel) MUST remove entries from `groups`.
    groups: HashMap<ShardGroupId, ShardGroupState>,
    /// The groups closed most recently, oldest first: those of downloads
    /// that ended, completed, failed or cancelled. A file of one that lands
    /// afterwards is ignored.
    closed: VecDeque<ShardGroupId>,
}

impl ShardGroupTracker {
    /// Create a new empty tracker.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record a shard completion.
    ///
    /// Returns `Some(GroupComplete)` if this was the last shard needed.
    /// This method is idempotent - recording the same shard index twice
    /// will not cause issues. A file of a group that was closed is ignored:
    /// it does not open the group again.
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
    /// In debug builds, panics if metadata for this group doesn't match the metadata
    /// already recorded. This catches bugs where shards compute different identities.
    pub(crate) fn on_shard_done(
        &mut self,
        group_id: &ShardGroupId,
        index: u32,
        path: PathBuf,
        expected_total: u32,
        metadata: &GroupMetadata,
    ) -> Option<GroupComplete> {
        // A closed group stays closed. Group ids are never reused, so this
        // can only be a file of the download that ended.
        if self.closed.contains(group_id) {
            return None;
        }

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

    /// Close a group whose download has ended, however it ended: forget
    /// what it had, and remember that it is closed.
    pub(crate) fn close(&mut self, group_id: &ShardGroupId) {
        self.groups.remove(group_id);
        if !self.closed.contains(group_id) {
            self.closed.push_back(group_id.clone());
        }
        let excess = self.closed.len().saturating_sub(CLOSED_LIMIT);
        self.closed.drain(..excess);
    }

    /// Check if there are any in-progress shard groups.
    ///
    /// Returns `true` if any shard groups are still incomplete (waiting for shards).
    /// This is used for drain detection: the queue is only truly drained if both
    /// the pending queue is empty AND `has_open_groups()` returns `false`.
    ///
    /// INVARIANT: This relies on terminal paths removing groups from `self.groups`.
    pub(crate) fn has_open_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Whether this group has a file in and is waiting for the rest.
    pub(crate) fn is_open(&self, group_id: &ShardGroupId) -> bool {
        self.groups.contains_key(group_id)
    }

    /// Get the number of active shard groups.
    #[cfg(test)]
    pub(crate) fn active_count(&self) -> usize {
        self.groups.len()
    }
}

#[cfg(test)]
#[path = "shard_group_tracker_tests.rs"]
mod tests;
