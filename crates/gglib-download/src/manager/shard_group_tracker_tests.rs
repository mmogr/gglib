//! Tests for the shard group tracker.

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

/// A group is open from its first file until its last, and one group being
/// open says nothing of another.
#[test]
fn a_group_is_open_from_its_first_file_to_its_last() {
    let mut tracker = ShardGroupTracker::new();
    let group = ShardGroupId::new("group-a");
    let other = ShardGroupId::new("group-b");
    let metadata = test_metadata();
    assert!(!tracker.is_open(&group));

    tracker.on_shard_done(&group, 0, PathBuf::from("/a0"), 2, &metadata);
    assert!(tracker.is_open(&group));
    assert!(!tracker.is_open(&other));

    tracker.on_shard_done(&group, 1, PathBuf::from("/a1"), 2, &metadata);
    assert!(!tracker.is_open(&group));
}
