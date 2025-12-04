//! Child process management for downloads.
//!
//! Tracks PIDs of download subprocess (Python huggingface-cli) for
//! synchronous termination on app shutdown.
//!
//! # Design
//!
//! - Uses `std::sync::RwLock` (not tokio) for synchronous access in shutdown handler.
//! - `kill_all_sync` can be called without an async runtime.
//! - Platform-specific process termination (SIGKILL on Unix, TerminateProcess on Windows).

use std::collections::HashMap;
use std::sync::Arc;

/// Type alias for storing child process PIDs for synchronous termination on shutdown.
///
/// Uses `std::sync::RwLock` (not tokio) for synchronous access in the shutdown handler.
pub type PidStorage = Arc<std::sync::RwLock<HashMap<String, u32>>>;

/// Manages child process PIDs for download operations.
///
/// This is a sync type that can be shared across async tasks.
/// It provides synchronous process termination for app shutdown.
#[derive(Clone)]
pub struct DownloadProcessManager {
    /// Maps tracking keys to process IDs.
    /// Key format: "model_id" or "model_id:quantization/filename" for shards.
    pids: PidStorage,
}

impl DownloadProcessManager {
    /// Create a new process manager.
    pub fn new() -> Self {
        Self {
            pids: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Get a clone of the PID storage for passing to download functions.
    ///
    /// This allows download functions to register their child process PIDs
    /// so they can be killed synchronously on app shutdown.
    pub fn pid_storage(&self) -> PidStorage {
        self.pids.clone()
    }

    /// Register a child process PID.
    ///
    /// # Arguments
    ///
    /// * `key` - Tracking key (e.g., "model_id" or "model_id:quant/filename")
    /// * `pid` - Process ID of the child process
    pub fn register(&self, key: String, pid: u32) {
        if let Ok(mut guard) = self.pids.write() {
            guard.insert(key, pid);
        }
    }

    /// Unregister a child process PID.
    ///
    /// Call this when a download completes (successfully or with error).
    pub fn unregister(&self, key: &str) {
        if let Ok(mut guard) = self.pids.write() {
            guard.remove(key);
        }
    }

    /// Get the number of tracked processes.
    pub fn count(&self) -> usize {
        self.pids
            .read()
            .map(|guard| guard.len())
            .unwrap_or(0)
    }

    /// Synchronously kill all active child processes.
    ///
    /// This method is designed for app shutdown and does NOT require an async runtime.
    /// It sends SIGKILL (Unix) or TerminateProcess (Windows) to all tracked child processes.
    ///
    /// # Returns
    ///
    /// Returns the number of processes that were signaled to terminate.
    pub fn kill_all_sync(&self) -> usize {
        use tracing::{debug, info};

        // Get all PIDs - use write lock to drain the map
        let pids: Vec<(String, u32)> = match self.pids.write() {
            Ok(mut guard) => {
                debug!(tracked_count = guard.len(), "Draining PID storage");
                guard.drain().collect()
            }
            Err(poisoned) => {
                info!("PID storage lock was poisoned, recovering");
                poisoned.into_inner().drain().collect()
            }
        };

        let count = pids.len();
        if count == 0 {
            debug!("No child processes tracked - nothing to kill");
            return 0;
        }

        info!(count = count, "Killing all child processes for shutdown");

        for (key, pid) in &pids {
            info!(key = %key, pid = pid, "Sending termination signal to process");
            kill_process_by_pid(*pid);
        }

        info!(count = count, "Process termination signals sent");
        count
    }
}

impl Default for DownloadProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Kill a process by PID using platform-specific APIs.
///
/// This is a synchronous function that sends a termination signal to the process.
/// On Unix (macOS/Linux), it sends SIGKILL. On Windows, it uses TerminateProcess.
///
/// # Arguments
///
/// * `pid` - The process ID to terminate
#[cfg(unix)]
pub fn kill_process_by_pid(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use tracing::warn;

    let nix_pid = Pid::from_raw(pid as i32);
    if let Err(e) = kill(nix_pid, Signal::SIGKILL) {
        // ESRCH means process doesn't exist (already terminated) - that's fine
        if e != nix::errno::Errno::ESRCH {
            warn!(pid = pid, error = %e, "Failed to kill process");
        }
    }
}

#[cfg(windows)]
pub fn kill_process_by_pid(pid: u32) {
    use tracing::warn;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid);
        match handle {
            Ok(h) => {
                if let Err(e) = TerminateProcess(h, 1) {
                    warn!(pid = pid, error = ?e, "Failed to terminate process");
                }
                let _ = CloseHandle(h);
            }
            Err(e) => {
                // ERROR_INVALID_PARAMETER means process doesn't exist - that's fine
                warn!(pid = pid, error = ?e, "Failed to open process for termination");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_manager_new() {
        let manager = DownloadProcessManager::new();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_register_and_count() {
        let manager = DownloadProcessManager::new();
        manager.register("model/test".to_string(), 12345);
        assert_eq!(manager.count(), 1);

        manager.register("model/test2".to_string(), 12346);
        assert_eq!(manager.count(), 2);
    }

    #[test]
    fn test_unregister() {
        let manager = DownloadProcessManager::new();
        manager.register("model/test".to_string(), 12345);
        assert_eq!(manager.count(), 1);

        manager.unregister("model/test");
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_unregister_nonexistent() {
        let manager = DownloadProcessManager::new();
        manager.unregister("nonexistent"); // Should not panic
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_pid_storage_clone() {
        let manager = DownloadProcessManager::new();
        let storage = manager.pid_storage();

        // Register via manager
        manager.register("key1".to_string(), 100);

        // Should be visible via storage clone
        let guard = storage.read().unwrap();
        assert_eq!(guard.get("key1"), Some(&100));
    }

    #[test]
    fn test_kill_all_sync_empty() {
        let manager = DownloadProcessManager::new();
        let killed = manager.kill_all_sync();
        assert_eq!(killed, 0);
    }

    #[test]
    fn test_kill_all_sync_drains() {
        let manager = DownloadProcessManager::new();
        // Register fake PIDs (won't actually exist, but tests the drain logic)
        manager.register("model/a".to_string(), 999999);
        manager.register("model/b".to_string(), 999998);

        assert_eq!(manager.count(), 2);
        let killed = manager.kill_all_sync();
        assert_eq!(killed, 2);
        assert_eq!(manager.count(), 0); // Drained
    }

    #[test]
    fn test_manager_is_clone() {
        let manager1 = DownloadProcessManager::new();
        let manager2 = manager1.clone();

        manager1.register("key1".to_string(), 100);
        assert_eq!(manager2.count(), 1); // Shared state
    }
}
