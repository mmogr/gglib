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
/// Uses `is_our_llama_server()` to avoid killing unrelated processes.
/// If verification fails, only the PID file is removed (conservative).
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

    #[tokio::test]
    async fn cleanup_removes_stale_pidfiles() {
        // Create PID file for impossible PID
        write_pidfile(99999, 999999, 9999).expect("write failed");

        cleanup_orphaned_servers().await.expect("cleanup failed");

        // Should have been removed
        let pidfiles = list_pidfiles().expect("list failed");
        assert!(!pidfiles.iter().any(|(id, _)| *id == 99999));
    }
}
