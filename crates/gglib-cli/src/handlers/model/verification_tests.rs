//! Tests for what `gglib model verify` prints: the verdict's line, how many
//! files are damaged, a row per file and each file's health under
//! `--per-shard`, in the words gglib-core writes; and when it exits 1.

use gglib_core::services::{
    Damage, OverallHealth, ShardHealth, ShardHealthReport, VerificationReport, VerifiedFileKind,
};

use super::{fails, file_status, overall_line, summary, summary_row};

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

fn shard(index: usize, path: &str, health: ShardHealth) -> ShardHealthReport {
    ShardHealthReport {
        index,
        kind: VerifiedFileKind::Shard {
            number: index + 1,
            of: 2,
        },
        file_path: path.to_owned(),
        health,
    }
}

/// A report as the daemon makes one of these files.
fn report(shards: Vec<ShardHealthReport>) -> VerificationReport {
    let healths: Vec<_> = shards.iter().map(|s| s.health.clone()).collect();
    VerificationReport {
        model_id: 1,
        overall_health: OverallHealth::of(&healths),
        damage: Damage::of(&shards),
        shards,
        verified_at: chrono::Utc::now(),
    }
}

#[test]
fn a_healthy_check_prints_its_marks_and_exits_0() {
    let r = report(vec![
        shard(0, "m-1.gguf", ShardHealth::Healthy),
        shard(1, "m-2.gguf", ShardHealth::NoOid),
    ]);
    assert_eq!(
        summary(&r),
        "Overall Health: ✓ Healthy\n\n  ✓ Shard 1/2: m-1.gguf (file 0)\n  ⚠ Shard 2/2: m-2.gguf (file 1)\n"
    );
    assert!(!fails(&r.overall_health));
}

#[test]
fn an_unverifiable_check_prints_its_marks_and_exits_0() {
    let r = report(vec![
        shard(0, "m-1.gguf", ShardHealth::NoOid),
        shard(1, "m-2.gguf", ShardHealth::NoOid),
    ]);
    assert_eq!(
        summary(&r),
        "Overall Health: ⚠ Unverifiable\n\n  ⚠ Shard 1/2: m-1.gguf (file 0)\n  ⚠ Shard 2/2: m-2.gguf (file 1)\n"
    );
    assert!(!fails(&r.overall_health));
}

/// The verdict's line says how many files are damaged, a corrupt file's
/// hashes follow its row, and the command exits 1.
#[test]
fn an_unhealthy_check_counts_the_damage_and_exits_1() {
    let corrupt = ShardHealth::Corrupt {
        expected: "e".repeat(8),
        actual: "a".repeat(8),
    };
    let r = report(vec![
        shard(0, "m-1.gguf", corrupt),
        shard(1, "m-2.gguf", ShardHealth::Missing),
    ]);
    assert_eq!(
        summary(&r),
        "Overall Health: ✗ Unhealthy · 2 files damaged\n\n  ✗ Shard 1/2: m-1.gguf (file 0)\n      Expected: eeeeeeee\n      Actual:   aaaaaaaa\n  ✗ Shard 2/2: m-2.gguf (file 1)\n"
    );
    assert!(fails(&r.overall_health));
}

#[test]
fn a_summary_row_names_the_file_by_kind_path_and_index() {
    assert_eq!(
        summary_row(&shard(1, "m-2.gguf", ShardHealth::Missing)),
        "  ✗ Shard 2/2: m-2.gguf (file 1)"
    );
}
