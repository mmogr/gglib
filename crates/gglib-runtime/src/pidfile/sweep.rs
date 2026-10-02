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
///    - If verified, kill it with `kill_pid`: SIGTERM → SIGKILL on Unix,
///      `taskkill /F` on Windows
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
///
/// An isolated data root does not make an unlocked call safe either: anything
/// else that writes pidfiles into the same root is the live sibling the lock
/// guards against. That is another process given the same `GGLIB_DATA_DIR`,
/// or, under `gglib_core`'s test root, another test in the same binary. So the
/// test that calls this without the lock is the only test in its binary
/// (`tests/pidfile_sweep.rs`).
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
                Ok(()) => {
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
