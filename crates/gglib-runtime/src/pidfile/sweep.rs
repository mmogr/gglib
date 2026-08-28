//! Startup orphan cleanup for llama-server processes from previous crashes.

use std::io;

use tracing::{debug, info, warn};

use super::io::{delete_pidfile, list_pidfiles};
use super::verify::is_our_llama_server;
use crate::process::shutdown::kill_pid;

/// Clean up orphaned llama-server processes at startup.
///
/// # Strategy
/// 1. Read all PID files from `~/.gglib/pids/`
/// 2. For each PID:
///    - Verify it's actually our llama-server binary (not a reused PID)
///    - If verified, kill it with SIGTERM → SIGKILL
///    - If not verified or already gone, just delete the PID file
/// 3. Log results
///
/// # Safety
///
/// **The caller must hold the daemon lock.** `is_our_llama_server()` stops this
/// killing *unrelated* processes; it does nothing about killing a *live* one
/// that belongs to somebody else, because verification succeeding is exactly
/// what a running sibling looks like. The lock is what makes "recorded pid" and
/// "dead process" the same thing. `daemon::run_daemon` acquires it before
/// calling here, and its comment records that this sweep once lived in the
/// desktop app's startup where it killed servers a concurrent CLI had just
/// spawned.
pub async fn cleanup_orphaned_servers() -> io::Result<()> {
    let pidfiles = list_pidfiles()?;

    if pidfiles.is_empty() {
        debug!("No orphaned PID files found");
        return Ok(());
    }

    info!(
        "Found {} PID files, checking for orphaned servers",
        pidfiles.len()
    );

    let mut killed = 0;
    let mut cleaned = 0;

    for (model_id, data) in pidfiles {
        if is_our_llama_server(data.pid) {
            // Verified orphaned server - kill it
            debug!(
                "Killing orphaned llama-server (model {}, PID {}, port {})",
                model_id, data.pid, data.port
            );

            match kill_pid(data.pid).await {
                Ok(_) => {
                    killed += 1;
                    delete_pidfile(model_id)?;
                }
                Err(e) => {
                    warn!(
                        "Failed to kill orphaned server PID {}: {}. Removing stale PID file.",
                        data.pid, e
                    );
                    delete_pidfile(model_id)?;
                    cleaned += 1;
                }
            }
        } else {
            // PID doesn't match our binary (reused or gone) - just clean up file
            debug!(
                "PID {} (model {}) is not our llama-server, removing stale PID file",
                data.pid, model_id
            );
            delete_pidfile(model_id)?;
            cleaned += 1;
        }
    }

    if killed > 0 || cleaned > 0 {
        info!(
            "Orphan cleanup complete: {} servers killed, {} stale files removed",
            killed, cleaned
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pidfile::io::write_pidfile;

    /// Drives the real sweep against the real `pids_dir()`, so it is `#[ignore]`d.
    ///
    /// In a debug build `detect_local_repo` returns the checkout unconditionally
    /// (`gglib_core::paths::platform`), so `pids_dir()` is `<repo>/pids` — the
    /// same directory a developer's installed daemon writes to. The sweep reads
    /// *every* pidfile there, and `is_our_llama_server` matches on the canonical
    /// exe path, which a real `<repo>/.llama/bin/llama-server` satisfies exactly.
    /// Running this with a model resident therefore SIGTERMs it and deletes its
    /// pidfile: observed on 2026-08-28, where a `cargo test --workspace` killed a
    /// live 27B server and the suite passed while doing it.
    ///
    /// Its sibling `list_pidfiles_filters_non_pid_files` in `io.rs` is `#[ignore]`d
    /// for a related reason — it deletes every `.pid` in the real directory
    /// before writing its own.
    ///
    /// Isolating properly wants `GGLIB_DATA_DIR`. Setting an env var needs
    /// `unsafe`, which the workspace denies — but it does **not** foreclose the
    /// route: `gglib_core::paths::test_utils` already carries an `EnvVarGuard`
    /// behind `#[allow(unsafe_code)]`, serialized by its own lock and in use
    /// today. It is `pub(super)`, so reaching it from here means exporting it
    /// behind a `test-utils` feature, the shape `gglib-db` is already consumed
    /// with. That is the fix; this is only the stop. Tracked as #955.
    /// A model id no real catalog hands out, and one no sibling test uses.
    ///
    /// Both matter. `pids_dir()` is shared with the developer's own daemon, so a
    /// plausible id would collide with a real model's pidfile — the reason
    /// `process::residency::launch_tests` reaches for the `999_00x` range. And
    /// `io.rs`'s ignored sibling used `99999` too, so under `--ignored` the two
    /// raced on one file and the run failed 3/3.
    const SWEEP_ID: i64 = 999_010;

    #[tokio::test]
    #[ignore = "drives the real pidfile sweep; kills a live llama-server if one is resident"]
    async fn cleanup_removes_stale_pidfiles() {
        // A pid nothing can own, so the unverified branch is the one exercised.
        write_pidfile(SWEEP_ID, 999_999, 9999).expect("write failed");

        cleanup_orphaned_servers().await.expect("cleanup failed");

        // Should have been removed
        let pidfiles = list_pidfiles().expect("list failed");
        assert!(!pidfiles.iter().any(|(id, _)| *id == SWEEP_ID));
    }
}
