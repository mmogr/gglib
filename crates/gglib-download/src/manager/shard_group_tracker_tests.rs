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
fn test_close_cleanup() {
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
    tracker.close(&group_id);

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
    tracker.close(&group_id);

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
    tracker.close(&group_b);
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

/// A file of a closed group that lands late does not open the group again,
/// even when it is the group's last. Another group is untouched.
#[test]
fn a_late_file_does_not_reopen_a_closed_group() {
    let mut tracker = ShardGroupTracker::new();
    let group = ShardGroupId::new("group-a");
    let other = ShardGroupId::new("group-b");
    let metadata = test_metadata();
    tracker.on_shard_done(&group, 0, PathBuf::from("/a0"), 2, &metadata);

    tracker.close(&group);
    let late = tracker.on_shard_done(&group, 1, PathBuf::from("/a1"), 2, &metadata);

    assert!(late.is_none());
    assert!(!tracker.is_open(&group));
    assert!(!tracker.has_open_groups());

    // A group closed before any file of it landed is closed all the same.
    tracker.close(&other);
    tracker.on_shard_done(&other, 0, PathBuf::from("/b0"), 2, &metadata);
    assert!(!tracker.has_open_groups());
}

/// The tracker remembers a bounded number of closed groups, the latest.
#[test]
fn the_closed_groups_remembered_are_bounded() {
    let mut tracker = ShardGroupTracker::new();
    let metadata = test_metadata();
    let group = |n: usize| ShardGroupId::new(format!("group-{n}"));

    for n in 0..=CLOSED_LIMIT {
        tracker.close(&group(n));
        tracker.close(&group(n));
    }

    assert_eq!(tracker.closed.len(), CLOSED_LIMIT);
    tracker.on_shard_done(&group(CLOSED_LIMIT), 0, PathBuf::from("/s"), 2, &metadata);
    assert!(!tracker.has_open_groups(), "the latest is remembered");
    tracker.on_shard_done(&group(0), 0, PathBuf::from("/s"), 2, &metadata);
    assert!(tracker.is_open(&group(0)), "the oldest has made way");
}
