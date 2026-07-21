//! Byte-budget selection for on-disk KV slot cache eviction.
//!
//! Pure decision logic, no IO: given the current set of cached slot files and
//! a byte budget, decide which files to delete. The disk-scanning and
//! deletion itself lives in `gglib-proxy` (`slot_eviction.rs`), which stats
//! the slot directory and hands the results here.

use std::path::PathBuf;

/// Metadata for one on-disk slot file, gathered by the IO layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFileMeta {
    pub path: PathBuf,
    pub mtime_unix_secs: u64,
    pub len_bytes: u64,
}

/// Given all cached slot files and a byte budget, return the paths to delete.
///
/// Deletes oldest-`mtime`-first until the surviving total is `<= budget_bytes`.
/// Ties on `mtime` break on `path` so the result is deterministic. A single
/// file larger than the entire budget is still evicted (an empty cache is a
/// valid outcome; a cache permanently over budget is not).
#[must_use]
pub fn select_evictions(mut files: Vec<SlotFileMeta>, budget_bytes: u64) -> Vec<PathBuf> {
    files.sort_by(|a, b| {
        a.mtime_unix_secs
            .cmp(&b.mtime_unix_secs)
            .then_with(|| a.path.cmp(&b.path))
    });

    let total: u64 = files.iter().map(|f| f.len_bytes).sum();
    let mut remaining = total;
    let mut evicted = Vec::new();

    for file in files {
        if remaining <= budget_bytes {
            break;
        }
        remaining = remaining.saturating_sub(file.len_bytes);
        evicted.push(file.path);
    }

    evicted
}

/// Divisor applied to (free disk space + cache footprint) for the auto budget.
///
/// Recomputed on every sweep so it tracks disk pressure from other
/// applications, not just this cache's own growth.
pub const DISK_BUDGET_FRACTION_DIVISOR: u64 = 4;

/// Compute an auto-sized disk budget, in bytes.
///
/// `available_bytes` is free space on the filesystem holding the slot
/// directory; `current_cache_bytes` is the cache's own current footprint
/// (already-cached files count as "available" for the cache to keep using,
/// since evicting them doesn't free space for anything else). The budget is
/// a quarter of that combined figure — safe headroom for the rest of the
/// disk, expanding automatically as free space changes.
#[must_use]
pub const fn compute_auto_disk_budget_bytes(available_bytes: u64, current_cache_bytes: u64) -> u64 {
    available_bytes.saturating_add(current_cache_bytes) / DISK_BUDGET_FRACTION_DIVISOR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(path: &str, mtime: u64, len: u64) -> SlotFileMeta {
        SlotFileMeta {
            path: PathBuf::from(path),
            mtime_unix_secs: mtime,
            len_bytes: len,
        }
    }

    #[test]
    fn under_budget_evicts_nothing() {
        let files = vec![meta("a.bin", 1, 100), meta("b.bin", 2, 100)];
        assert!(select_evictions(files, 1000).is_empty());
    }

    #[test]
    fn exact_budget_boundary_evicts_nothing() {
        let files = vec![meta("a.bin", 1, 100), meta("b.bin", 2, 100)];
        assert!(select_evictions(files, 200).is_empty());
    }

    #[test]
    fn evicts_oldest_first_until_under_budget() {
        let files = vec![
            meta("oldest.bin", 1, 100),
            meta("middle.bin", 2, 100),
            meta("newest.bin", 3, 100),
        ];
        // total 300, budget 150 -> evict oldest (200 left), then middle (100 left)
        let evicted = select_evictions(files, 150);
        assert_eq!(
            evicted,
            vec![PathBuf::from("oldest.bin"), PathBuf::from("middle.bin")]
        );
    }

    #[test]
    fn zero_budget_evicts_everything() {
        let files = vec![meta("a.bin", 1, 100), meta("b.bin", 2, 100)];
        let evicted = select_evictions(files, 0);
        assert_eq!(evicted.len(), 2);
    }

    #[test]
    fn single_file_larger_than_budget_is_evicted() {
        let files = vec![meta("huge.bin", 1, 10_000)];
        assert_eq!(
            select_evictions(files, 100),
            vec![PathBuf::from("huge.bin")]
        );
    }

    #[test]
    fn mtime_ties_break_on_path() {
        let files = vec![
            meta("z.bin", 5, 100),
            meta("a.bin", 5, 100),
            meta("m.bin", 5, 100),
        ];
        // all same mtime, budget forces evicting two -> lexicographically first two
        let evicted = select_evictions(files, 100);
        assert_eq!(
            evicted,
            vec![PathBuf::from("a.bin"), PathBuf::from("m.bin")]
        );
    }

    #[test]
    fn auto_disk_budget_is_a_quarter_of_available_plus_cache() {
        // 40 GiB free + 8 GiB already cached -> 12 GiB budget
        let available = 40 * 1024 * 1024 * 1024;
        let cache = 8 * 1024 * 1024 * 1024;
        assert_eq!(
            compute_auto_disk_budget_bytes(available, cache),
            12 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn auto_disk_budget_saturates_on_overflow() {
        // available + cache overflows u64 and saturates to u64::MAX before dividing.
        assert_eq!(
            compute_auto_disk_budget_bytes(u64::MAX, u64::MAX),
            u64::MAX / DISK_BUDGET_FRACTION_DIVISOR
        );
    }
}
