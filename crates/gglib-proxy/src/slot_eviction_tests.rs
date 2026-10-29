//! Tests for [`super`]: the budget a sweep resolves, what it evicts and
//! reaps, and that its task stops on cancel.
//!
//! Declared as `mod tests` through `#[path]`, so every test keeps the
//! `slot_eviction::tests::` path it had when it lived inline.

use super::*;
use gglib_core::paths::slot_bin_path;
use std::sync::Arc;
use std::time::SystemTime;

fn set_mtime(path: &Path, secs_ago: u64) {
    let mtime = SystemTime::now() - Duration::from_secs(secs_ago);
    let file = std::fs::File::open(path).unwrap();
    file.set_modified(mtime).unwrap();
}

#[test]
fn resolve_disk_budget_explicit_flag_wins() {
    let budget = resolve_disk_budget_inner(Some(10), Some("999".to_string()));
    assert_eq!(budget, DiskBudget::ExplicitBytes(10 * 1024 * 1024 * 1024));
}

#[test]
fn resolve_disk_budget_env_var_when_no_flag() {
    let budget = resolve_disk_budget_inner(None, Some("20".to_string()));
    assert_eq!(budget, DiskBudget::ExplicitBytes(20 * 1024 * 1024 * 1024));
}

#[test]
fn resolve_disk_budget_auto_when_neither_set() {
    assert_eq!(resolve_disk_budget_inner(None, None), DiskBudget::Auto);
}

#[test]
fn resolve_disk_budget_auto_on_unparseable_env() {
    assert_eq!(
        resolve_disk_budget_inner(None, Some("not-a-number".to_string())),
        DiskBudget::Auto
    );
}

#[tokio::test]
async fn evict_over_budget_deletes_oldest_first_to_explicit_budget() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();

    let oldest = slot_bin_path(d, 1, "oldest");
    let middle = slot_bin_path(d, 1, "middle");
    let newest = slot_bin_path(d, 1, "newest");
    std::fs::write(&oldest, vec![0u8; 100]).unwrap();
    std::fs::write(&middle, vec![0u8; 100]).unwrap();
    std::fs::write(&newest, vec![0u8; 100]).unwrap();
    set_mtime(&oldest, 300);
    set_mtime(&middle, 200);
    set_mtime(&newest, 100);

    // total 300 bytes, budget 250 -> evict oldest only (200 <= 250, stop)
    evict_over_budget(d, DiskBudget::ExplicitBytes(250), &HostFacts::none())
        .await
        .unwrap();

    assert!(!oldest.exists());
    assert!(middle.exists());
    assert!(newest.exists());
}

#[tokio::test]
async fn evict_over_budget_spares_fresh_tmp_file() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let tmp = d.join("1__inflight.0.tmp");
    std::fs::write(&tmp, b"x").unwrap();

    evict_over_budget(d, DiskBudget::ExplicitBytes(u64::MAX), &HostFacts::none())
        .await
        .unwrap();

    assert!(tmp.exists(), "fresh tmp file must survive a sweep");
}

#[tokio::test]
async fn evict_over_budget_reaps_stale_tmp_file() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let tmp = d.join("1__orphan.0.tmp");
    std::fs::write(&tmp, b"x").unwrap();
    set_mtime(&tmp, STALE_TMP_MAX_AGE.as_secs() + 60);

    evict_over_budget(d, DiskBudget::ExplicitBytes(u64::MAX), &HostFacts::none())
        .await
        .unwrap();

    assert!(!tmp.exists(), "stale orphaned tmp file must be reaped");
}

/// Four 100-byte slot files, oldest first, each 100 seconds younger than the
/// one before it.
fn four_slot_files(dir: &Path) -> Vec<PathBuf> {
    [("a", 400), ("b", 300), ("c", 200), ("d", 100)]
        .into_iter()
        .map(|(session, secs_ago)| {
            let path = slot_bin_path(dir, 1, session);
            std::fs::write(&path, vec![0u8; 100]).unwrap();
            set_mtime(&path, secs_ago);
            path
        })
        .collect()
}

/// With no free-space reader there is no Auto budget: every slot file stays,
/// and the sweep still reaps a stale `.tmp` file.
#[tokio::test]
async fn auto_budget_with_no_free_disk_reader_evicts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let files = four_slot_files(dir.path());
    let tmp = dir.path().join("1__orphan.0.tmp");
    std::fs::write(&tmp, b"x").unwrap();
    set_mtime(&tmp, STALE_TMP_MAX_AGE.as_secs() + 60);

    evict_over_budget(dir.path(), DiskBudget::Auto, &HostFacts::none())
        .await
        .unwrap();

    assert!(files.iter().all(|f| f.exists()));
    assert!(!tmp.exists(), "stale orphaned tmp file must be reaped");
}

/// A reader that cannot tell counts as no free space: a quarter of the
/// cache's own 400 bytes, so the three oldest files go.
#[tokio::test]
async fn auto_budget_counts_a_reader_that_cannot_tell_as_no_free_disk() {
    let dir = tempfile::tempdir().unwrap();
    let files = four_slot_files(dir.path());
    let host = HostFacts {
        free_disk: Some(Arc::new(|_: &Path| None)),
        ..HostFacts::none()
    };

    evict_over_budget(dir.path(), DiskBudget::Auto, &host)
        .await
        .unwrap();

    let kept: Vec<bool> = files.iter().map(|f| f.exists()).collect();
    assert_eq!(kept, [false, false, false, true]);
}

/// The reader is asked about the slot directory, and room on that disk
/// leaves the cache alone.
#[tokio::test]
async fn auto_budget_with_room_on_the_slot_dirs_disk_evicts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let files = four_slot_files(dir.path());
    let slot_dir = dir.path().to_path_buf();
    let host = HostFacts {
        free_disk: Some(Arc::new(move |path: &Path| {
            (path == slot_dir).then_some(1_000_000)
        })),
        ..HostFacts::none()
    };

    evict_over_budget(dir.path(), DiskBudget::Auto, &host)
        .await
        .unwrap();

    assert!(files.iter().all(|f| f.exists()));
}

#[tokio::test]
async fn spawn_eviction_task_exits_on_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let handle = spawn_eviction_task(
        dir.path().to_path_buf(),
        DiskBudget::Auto,
        HostFacts::none(),
        cancel.clone(),
    );

    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .expect("eviction task should exit promptly on cancellation")
        .expect("eviction task should not panic");
}
