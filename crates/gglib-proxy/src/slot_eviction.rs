//! Disk-aware byte-budget LRU eviction for the on-disk KV slot cache.
//!
//! Thin IO layer over [`gglib_core::domain::slot_eviction`]'s pure selection
//! logic: this module stats the slot directory, resolves the disk budget
//! (explicit override, or derived from the free space the caller's
//! [`HostFacts`] reads), and deletes what the pure selector says to. It also
//! reaps orphaned `*.tmp` files — the temp names
//! `slots::save_slot` writes to before an atomic rename, which are left
//! behind only when a save is interrupted or fails (see `slots.rs` module
//! docs for the rename protocol).

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use gglib_core::domain::slot_eviction::{
    SlotFileMeta, compute_auto_disk_budget_bytes, select_evictions,
};

use crate::host_facts::HostFacts;

/// Disk budget for cached slot files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskBudget {
    /// Derive a budget from free disk space at the slot directory (see
    /// [`compute_auto_disk_budget_bytes`]), recomputed every sweep. The free
    /// space is [`HostFacts::free_disk`]'s reading; a host with no reader has
    /// no Auto budget (see [`evict_over_budget`]).
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
const STALE_TMP_MAX_AGE: Duration = Duration::from_mins(15);

/// Background eviction task — spawned at server startup, runs every 60s.
///
/// Exits promptly on `cancel`, same shutdown contract as the other
/// background tasks (`spawn_slots_poller`, `spawn_dashboard_publisher`).
pub fn spawn_eviction_task(
    slot_dir: PathBuf,
    budget: DiskBudget,
    host: HostFacts,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_mins(1);
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                () = tokio::time::sleep(interval) => {
                    if let Err(e) = evict_over_budget(&slot_dir, budget, &host).await {
                        warn!("slot cache eviction sweep failed: {}", e);
                    }
                }
            }
        }
    })
}

/// Stat all `.bin` files under `slot_dir`, evict down to `budget`, then reap
/// stale orphaned `.tmp` files.
///
/// [`DiskBudget::Auto`] asks `host`'s free-disk reader about `slot_dir`. A
/// reader that cannot tell counts as no free space, so the budget is a quarter
/// of the cache's own footprint. A host with no reader has no Auto budget: the
/// sweep evicts no `.bin` file and only reaps stale `.tmp` files.
pub async fn evict_over_budget(
    slot_dir: &Path,
    budget: DiskBudget,
    host: &HostFacts,
) -> std::io::Result<()> {
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
        DiskBudget::Auto => host.free_disk.as_ref().map_or(u64::MAX, |read| {
            compute_auto_disk_budget_bytes(read(slot_dir).unwrap_or(0), total_bytes)
        }),
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
    let Ok(mut entries) = tokio::fs::read_dir(slot_dir).await else {
        return Ok(());
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("tmp") {
            continue;
        }
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        let Ok(age) = metadata
            .modified()
            .and_then(|m| m.elapsed().map_err(std::io::Error::other))
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
#[path = "slot_eviction_tests.rs"]
mod tests;
