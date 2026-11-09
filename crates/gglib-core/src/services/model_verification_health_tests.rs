//! Tests for a file check's verdict: each mix of file healths gives one
//! verdict, read in one word, and a model none of whose files has a hash
//! is unverifiable, never healthy.

use super::tests::{WEIGHTS, fixture, row, sha256};
use crate::domain::ModelFile;
use crate::ports::huggingface::fake_hub::FakeHub;
use crate::services::{
    Damage, OverallHealth, ShardHealth, ShardHealthReport, VerificationReport, VerifiedFileKind,
    damaged_line,
};

fn corrupt() -> ShardHealth {
    ShardHealth::Corrupt {
        expected: "a".repeat(64),
        actual: "b".repeat(64),
    }
}

/// The verdict on `files` and its word.
fn verdict(files: &[ShardHealth]) -> (OverallHealth, &'static str) {
    let overall = OverallHealth::of(files);
    (overall.clone(), overall.word())
}

#[test]
fn every_file_matching_is_healthy() {
    assert_eq!(
        verdict(&[ShardHealth::Healthy, ShardHealth::Healthy]),
        (OverallHealth::Healthy, "Healthy")
    );
}

#[test]
fn files_matching_beside_files_with_no_hash_are_healthy() {
    assert_eq!(
        verdict(&[ShardHealth::NoOid, ShardHealth::Healthy, ShardHealth::NoOid]),
        (OverallHealth::Healthy, "Healthy")
    );
}

#[test]
fn no_file_with_a_hash_is_unverifiable() {
    assert_eq!(
        verdict(&[ShardHealth::NoOid, ShardHealth::NoOid]),
        (OverallHealth::Unverifiable, "Unverifiable")
    );
    assert_eq!(verdict(&[]), (OverallHealth::Unverifiable, "Unverifiable"));
}

#[test]
fn a_corrupt_or_missing_file_is_unhealthy_whatever_else_there_is() {
    for files in [
        vec![ShardHealth::Healthy, corrupt()],
        vec![ShardHealth::Missing, ShardHealth::Healthy],
        vec![ShardHealth::NoOid, ShardHealth::Missing],
        vec![corrupt(), ShardHealth::NoOid],
    ] {
        assert_eq!(
            verdict(&files),
            (OverallHealth::Unhealthy, "Unhealthy"),
            "{files:?}"
        );
    }
}

#[test]
fn each_file_health_reads_as_its_word() {
    let words: Vec<_> = ShardHealth::all().iter().map(ShardHealth::word).collect();
    assert_eq!(words, ["Healthy", "Corrupt", "Missing", "No OID available"]);
    assert_eq!(corrupt().word(), "Corrupt");
}

#[test]
fn the_damaged_line_counts_its_files() {
    assert_eq!(damaged_line(0), None);
    assert_eq!(damaged_line(1).as_deref(), Some("1 file damaged"));
    assert_eq!(damaged_line(2).as_deref(), Some("2 files damaged"));
    assert_eq!(damaged_line(12).as_deref(), Some("12 files damaged"));
}

fn shard(index: usize, health: ShardHealth) -> ShardHealthReport {
    ShardHealthReport {
        index,
        kind: VerifiedFileKind::Projector,
        file_path: format!("file-{index}.gguf"),
        health,
    }
}

/// The damage is the corrupt and missing files, by index, and never a file
/// with no hash: the same files a repair fetches again.
#[test]
fn the_damage_is_the_corrupt_and_missing_files() {
    let shards = [
        shard(0, ShardHealth::Healthy),
        shard(1, corrupt()),
        shard(2, ShardHealth::NoOid),
        shard(3, ShardHealth::Missing),
    ];
    assert_eq!(
        Damage::of(&shards),
        Some(Damage {
            files: vec![1, 3],
            line: "2 files damaged".to_owned(),
        })
    );
    assert_eq!(
        Damage::of(&[shard(0, ShardHealth::NoOid), shard(1, ShardHealth::Healthy)]),
        None
    );
}

/// A row for `name` with no hash recorded.
fn unhashed(index: i32, name: &str) -> ModelFile {
    ModelFile {
        hf_oid: None,
        ..row(index, name, "")
    }
}

async fn report(dir: &std::path::Path, rows: Vec<ModelFile>) -> VerificationReport {
    let f = fixture(dir, rows, FakeHub::default());
    let (mut progress, verifying) = f.service.verify_model_integrity(1).await.unwrap();
    while progress.recv().await.is_some() {}
    verifying.await.unwrap().unwrap()
}

async fn verify(dir: &std::path::Path, rows: Vec<ModelFile>) -> OverallHealth {
    report(dir, rows).await.overall_health
}

/// One file with no hash, one whose hash is a Git SHA-1 and so none to
/// compare with: nothing is compared, and the check says so.
#[tokio::test]
async fn a_model_whose_files_have_no_hash_verifies_as_unverifiable() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(WEIGHTS), "weights").unwrap();
    std::fs::write(dir.path().join("zeta-extra.gguf"), "extra").unwrap();
    let rows = vec![
        unhashed(0, WEIGHTS),
        row(1, "zeta-extra.gguf", &"c".repeat(40)),
    ];

    assert_eq!(verify(dir.path(), rows).await, OverallHealth::Unverifiable);
}

#[tokio::test]
async fn a_model_with_one_file_matching_and_one_with_no_hash_verifies_as_healthy() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(WEIGHTS), "weights").unwrap();
    std::fs::write(dir.path().join("zeta-extra.gguf"), "extra").unwrap();
    let rows = vec![
        row(0, WEIGHTS, &sha256("weights")),
        unhashed(1, "zeta-extra.gguf"),
    ];

    assert_eq!(verify(dir.path(), rows).await, OverallHealth::Healthy);
}

/// A file not on disk beside one that matched: the report names the missing
/// one's index and says how many, and a clean check carries no damage.
#[tokio::test]
async fn a_report_carries_its_damaged_files_and_their_line() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(WEIGHTS), "weights").unwrap();
    let damaged = report(
        dir.path(),
        vec![
            row(0, WEIGHTS, &sha256("weights")),
            row(1, "zeta-extra.gguf", &sha256("extra")),
        ],
    )
    .await;
    assert_eq!(
        damaged.damage,
        Some(Damage {
            files: vec![1],
            line: "1 file damaged".to_owned(),
        })
    );

    let clean = report(dir.path(), vec![row(0, WEIGHTS, &sha256("weights"))]).await;
    assert_eq!(clean.damage, None);
}
