//! Tests for what `gglib model verify` prints: the verdict's line and each
//! file's health under `--per-shard`, in the words gglib-core writes.

use gglib_core::services::{OverallHealth, ShardHealth};

use super::{file_status, overall_line};

#[test]
fn the_verdict_line_says_each_verdict_with_its_mark() {
    assert_eq!(
        overall_line(&OverallHealth::Healthy),
        "Overall Health: ✓ Healthy"
    );
    assert_eq!(
        overall_line(&OverallHealth::Unhealthy),
        "Overall Health: ✗ Unhealthy"
    );
    assert_eq!(
        overall_line(&OverallHealth::Unverifiable),
        "Overall Health: ⚠ Unverifiable"
    );
}

/// A model none of whose files has a hash is unverifiable on the line, not
/// healthy.
#[test]
fn a_model_with_no_hash_to_compare_prints_unverifiable() {
    let files = [ShardHealth::NoOid, ShardHealth::NoOid];
    assert_eq!(
        overall_line(&OverallHealth::of(&files)),
        "Overall Health: ⚠ Unverifiable"
    );
    assert_eq!(file_status(&files[0]), "⚠ No OID available");
}

#[test]
fn a_damaged_model_prints_unhealthy_and_each_damaged_file() {
    let corrupt = ShardHealth::Corrupt {
        expected: "0123456789abcdef".repeat(4),
        actual: "fedcba9876543210".repeat(4),
    };
    let files = [ShardHealth::Healthy, corrupt.clone(), ShardHealth::Missing];
    assert_eq!(
        overall_line(&OverallHealth::of(&files)),
        "Overall Health: ✗ Unhealthy"
    );
    assert_eq!(
        file_status(&corrupt),
        "✗ Corrupt (expected: 01234567, actual: fedcba98)"
    );
    assert_eq!(file_status(&ShardHealth::Missing), "✗ Missing");
}

#[test]
fn a_healthy_model_prints_healthy_and_each_file_healthy() {
    let files = [ShardHealth::Healthy, ShardHealth::NoOid];
    assert_eq!(
        overall_line(&OverallHealth::of(&files)),
        "Overall Health: ✓ Healthy"
    );
    assert_eq!(file_status(&ShardHealth::Healthy), "✓ Healthy");
}
