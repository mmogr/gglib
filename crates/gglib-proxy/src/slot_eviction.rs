//! Disk-aware byte-budget LRU eviction for the on-disk KV slot cache.
//!
//! Thin IO layer over [`gglib_core::domain::slot_eviction`]'s pure selection
//! logic: this module stats the slot directory, resolves the disk budget
//! (explicit override or free-space-derived), and deletes what the pure
//! selector says to. It also reaps orphaned `*.tmp` files — the temp names
//! `slots::save_slot` writes to before an atomic rename, which are left
//! behind only when a save is interrupted or fails (see `slots.rs` module
//! docs for the rename protocol).

use std::path::{Path, PathBuf};
use std::time::Duration;

use sysinfo::Disks;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use gglib_core::domain::slot_eviction::{SlotFileMeta, compute_auto_disk_budget_bytes, select_evictions};

/// Disk budget for cached slot files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskBudget {
    /// Derive a budget from free disk space at the slot directory (see
    /// [`compute_auto_disk_budget_bytes`]), recomputed every sweep.
    Auto,
    /// Fixed byte budget, from `--cache-disk-gb` or `GGLIB_CACHE_DISK_GB`.
    ExplicitBytes(u64),
}

/// Env var for overriding the disk budget without a CLI flag (e.g. from a
/// `.env` file, matching the `GGLIB_DISABLE_<FEATURE>` convention used
/// elsewhere for cache knobs).
const DISK_BUDGET_ENV_VAR: &str = "GGLIB_CACHE_DISK_GB";

/// Resolve the disk budget: an explicit `--cache-disk-gb` flag wins, then
/// `GGLIB_CACHE_DISK_GB`, then [`DiskBudget::Auto`].
#[must_use]
pub fn resolve_disk_budget(explicit_gb: Option<u64>) -> DiskBudget {
    resolve_disk_budget_inner(explicit_gb, std::env::var(DISK_BUDGET_ENV_VAR).ok())
}

/// Pure core of [`resolve_disk_budget`], with the env lookup lifted into a
/// parameter so precedence is testable without mutating process-global
/// environment state.
fn resolve_disk_budget_inner(explicit_gb: Option<u64>, env_gb: Option<String>) -> DiskBudget {
    if let Some(gb) = explicit_gb {
        return DiskBudget::ExplicitBytes(gb.saturating_mul(1024 * 1024 * 1024));
    }
    if let Some(gb) = env_gb.and_then(|v| v.trim().parse::<u64>().ok()) {
        return DiskBudget::ExplicitBytes(gb.saturating_mul(1024 * 1024 * 1024));
    }
    DiskBudget::Auto
}

/// Tmp files older than this are orphans from a save that timed out, failed,
/// or was never confirmed — more than twice `slots::SAVE_TIMEOUT` so a
/// slow-but-still-in-flight save is never mistaken for an orphan.
const STALE_TMP_MAX_AGE: Duration = Duration::from_secs(15 * 60);

/// Background eviction task — spawned at server startup, runs every 60s.
/// Exits promptly on `cancel`, same shutdown contract as the other
/// background tasks (`spawn_slots_poller`, `spawn_dashboard_publisher`).
pub fn spawn_eviction_task(
    slot_dir: PathBuf,
    budget: DiskBudget,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(60);
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                () = tokio::time::sleep(interval) => {
                    if let Err(e) = evict_over_budget(&slot_dir, budget).await {
                        warn!("slot cache eviction sweep failed: {}", e);
                    }
                }
            }
        }
    })
}

/// Free bytes available on the filesystem holding `path`.
///
/// Matches `path` against every mounted disk's mount point and picks the
/// longest matching prefix (the most specific mount covering `path`).
/// Returns `0` if disk info can't be read — callers treat that as "no room",
/// which degrades the auto budget to `0` rather than silently over-granting.
fn available_disk_bytes(path: &Path) -> u64 {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map_or(0, sysinfo::Disk::available_space)
}

/// Stat all `.bin` files under `slot_dir`, evict down to `budget`, then reap
/// stale orphaned `.tmp` files.
pub async fn evict_over_budget(slot_dir: &Path, budget: DiskBudget) -> std::io::Result<()> {
    let mut files = Vec::new();
    let mut total_bytes: u64 = 0;
    for path in crate::slots::iter_all_slot_files(slot_dir).await {
        let Ok(metadata) = tokio::fs::metadata(&path).await else {
            continue;
        };
        let mtime_unix_secs = metadata
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        total_bytes = total_bytes.saturating_add(metadata.len());
        files.push(SlotFileMeta {
            path,
            mtime_unix_secs,
            len_bytes: metadata.len(),
        });
    }

    let budget_bytes = match budget {
        DiskBudget::ExplicitBytes(b) => b,
        DiskBudget::Auto => {
            compute_auto_disk_budget_bytes(available_disk_bytes(slot_dir), total_bytes)
        }
    };

    for path in select_evictions(files, budget_bytes) {
        let freed = tokio::fs::metadata(&path).await.map(|m| m.len()).ok();
        if let Err(e) = tokio::fs::remove_file(&path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            warn!("slot cache eviction failed for {}: {}", path.display(), e);
            continue;
        }
        info!(
            "evicted slot cache file {} ({} bytes) — over disk budget",
            path.display(),
            freed.unwrap_or(0)
        );
    }

    reap_stale_tmp_files(slot_dir).await
}

/// Remove orphaned `*.tmp` files older than [`STALE_TMP_MAX_AGE`] — leftovers
/// from a save that never completed its rename to the final `.bin` name.
async fn reap_stale_tmp_files(slot_dir: &Path) -> std::io::Result<()> {
    let mut entries = match tokio::fs::read_dir(slot_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("tmp") {
            continue;
        }
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        let Ok(age) = metadata.modified().and_then(|m| m.elapsed().map_err(std::io::Error::other))
        else {
            continue;
        };
        if age < STALE_TMP_MAX_AGE {
            continue;
        }
        if let Err(e) = tokio::fs::remove_file(&path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            warn!("failed to reap stale tmp file {}: {}", path.display(), e);
        } else {
            info!("reaped stale orphaned tmp file {}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::paths::slot_bin_path;
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
        evict_over_budget(d, DiskBudget::ExplicitBytes(250))
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

        evict_over_budget(d, DiskBudget::ExplicitBytes(u64::MAX))
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

        evict_over_budget(d, DiskBudget::ExplicitBytes(u64::MAX))
            .await
            .unwrap();

        assert!(!tmp.exists(), "stale orphaned tmp file must be reaped");
    }

    #[tokio::test]
    async fn spawn_eviction_task_exits_on_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let handle = spawn_eviction_task(dir.path().to_path_buf(), DiskBudget::Auto, cancel.clone());

        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("eviction task should exit promptly on cancellation")
            .expect("eviction task should not panic");
    }
}
