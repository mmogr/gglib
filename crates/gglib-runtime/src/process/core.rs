//! GUI-oriented process lifecycle management.
//!
//! This module provides process spawning, tracking, and management with
//! integrated log streaming and event broadcasting.
//!
//! [`GuiProcessCore`] keeps its `Gui` prefix from a time when a second,
//! port-aligned `ProcessCore` sat beside it in `process_core.rs` and
//! implemented a `ProcessRunner` trait for the CLI. Both are gone, though not
//! together: `process_core.rs` went in #708, and the trait outlived it until
//! #849, by which point nothing implemented it. This is now the only process
//! core, serving every caller rather than only the GUI.

use super::ports::{allocate_port, is_port_available};
use super::shutdown::shutdown_child;
use super::types::{RunningProcess, ServerInfo};
use crate::command::{build_and_spawn, spawn_log_readers};
use crate::pidfile::{delete_pidfile, write_pidfile};
use anyhow::{Result, anyhow};
use gglib_core::ports::ServerConfig;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// GUI-oriented process lifecycle manager.
///
/// Handles spawning, tracking, and killing llama-server processes with
/// integrated log streaming for GUI applications. Uses `u32` model IDs
/// for frontend compatibility.
///
/// The only process core. See the module docs above for why the name still
/// carries a `Gui` prefix.
pub struct GuiProcessCore {
    /// Running processes keyed by `model_id`
    processes: HashMap<u32, RunningProcess>,
    /// Base port for allocation
    base_port: u16,
    /// Path to llama-server binary
    llama_server_path: String,
}

impl GuiProcessCore {
    /// Create a new `GuiProcessCore`
    pub fn new(base_port: u16, llama_server_path: impl Into<String>) -> Self {
        Self {
            processes: HashMap::new(),
            base_port,
            llama_server_path: llama_server_path.into(),
        }
    }

    /// Spawn a new llama-server process
    ///
    /// Returns the port number for the spawned process.
    pub async fn spawn(&mut self, config: ServerConfig) -> Result<u16> {
        let model_id = config.model_id as u32;

        if self.processes.contains_key(&model_id) {
            return Err(anyhow!("Model {} is already running", model_id));
        }

        if !config.model_path.exists() {
            return Err(anyhow!(
                "Model file not found: {}",
                config.model_path.display()
            ));
        }

        let port = self.resolve_port(config.port)?;
        // TEMPORARY diagnostic for the proxy-dashboard port-mismatch bug
        // report (gglib PR #568) — logs the configured base port alongside
        // the port actually allocated for this spawn, so a future repro can
        // confirm whether `--llama-port` reached this point correctly.
        // Safe to remove once the bug is confirmed resolved or a root cause
        // is found elsewhere.
        tracing::info!(
            model_id = %model_id,
            base_port = %self.base_port,
            requested_port = ?config.port,
            allocated_port = %port,
            "GuiProcessCore::spawn allocated port"
        );
        let llama_path = Path::new(&self.llama_server_path);
        let mut child = build_and_spawn(Some(llama_path), &config, port)?;
        let pid = child
            .id()
            .ok_or_else(|| anyhow!("Failed to get child PID"))?;

        // Write PID file
        if let Err(e) = write_pidfile(config.model_id, pid, port) {
            debug!("Failed to write PID file: {}", e);
        }

        self.spawn_log_readers(&mut child, port);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let info = ServerInfo::new(model_id, config.model_name, pid, port, now);
        let running = RunningProcess::new(info, child);
        self.processes.insert(model_id, running);

        Ok(port)
    }

    fn spawn_log_readers(&self, child: &mut tokio::process::Child, port: u16) {
        use crate::process::LogManagerSink;
        spawn_log_readers(child, port, Some(Arc::new(LogManagerSink)));
    }

    fn resolve_port(&self, requested: Option<u16>) -> Result<u16> {
        match requested {
            Some(p) if p < 1024 => Err(anyhow!(
                "Port {} is a privileged port. Please use a port >= 1024.",
                p
            )),
            Some(p) if !is_port_available(p) => Err(anyhow!(
                "Port {} is already in use. Please choose a different port.",
                p
            )),
            Some(p) => Ok(p),
            None => {
                let used: Vec<u16> = self.processes.values().map(|p| p.info.port).collect();
                allocate_port(self.base_port, &used)
            }
        }
    }

    /// Kill a running process with graceful shutdown
    pub async fn kill(&mut self, model_id: u32) -> Result<()> {
        let running = self
            .processes
            .remove(&model_id)
            .ok_or_else(|| anyhow!("Model {} is not running", model_id))?;

        let pid = running.info.pid;
        debug!(model_id = %model_id, pid = %pid, port = %running.info.port, "Stopping process");

        // Use graceful shutdown with SIGTERM → SIGKILL
        let _ = shutdown_child(running.child).await;

        // Remove PID file
        if let Err(e) = delete_pidfile(model_id as i64) {
            debug!("Failed to delete PID file: {}", e);
        }

        Ok(())
    }

    /// List all running processes
    pub fn list_all(&self) -> Vec<&ServerInfo> {
        debug!(process_count = %self.processes.len(), "GuiProcessCore: list_all called");
        self.processes.values().map(|p| &p.info).collect()
    }

    /// Check if a model is running.
    ///
    /// Test-only: its production caller was `ProcessManager::is_serving`,
    /// removed in this commit. Gated so `dead_code` keeps telling the truth
    /// about production reach.
    #[cfg(test)]
    pub(crate) fn is_running(&self, model_id: u32) -> bool {
        self.processes.contains_key(&model_id)
    }

    /// Get count of running processes.
    ///
    /// Test-only, and already so before this commit — nothing outside the test
    /// below calls it. Gated for the same reason as [`Self::is_running`].
    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.processes.len()
    }

    /// Remove dead processes from tracking and clean PID files
    pub async fn cleanup_dead(&mut self) -> Vec<u32> {
        debug!(process_count = %self.processes.len(), "cleanup_dead called");
        let mut dead = Vec::new();

        for (id, running) in self.processes.iter_mut() {
            match running.child.try_wait() {
                Ok(Some(status)) => {
                    debug!(id = %id, status = ?status, "Process exited");
                    dead.push(*id);
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(id = %id, error = %e, "Error checking process");
                    dead.push(*id);
                }
            }
        }

        for id in &dead {
            debug!(id = %id, "Removing dead process from map");
            self.processes.remove(id);
            // Remove PID file for naturally exited process
            if let Err(e) = delete_pidfile(*id as i64) {
                debug!("Failed to delete PID file for {}: {}", id, e);
            }
        }

        debug!(removed_count = %dead.len(), remaining_count = %self.processes.len(), "cleanup_dead finished");
        dead
    }
}

// Note: Drop is not async, so the SIGTERM-then-SIGKILL sequence `kill` uses
// cannot run here. This is a backstop that kills outright; anything wanting a
// graceful stop has to go through `kill` before the core is dropped.
impl Drop for GuiProcessCore {
    fn drop(&mut self) {
        // Best effort: just kill the child handles
        for (_, running) in self.processes.drain() {
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(running.info.pid.to_string())
                .output();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_creation() {
        let core = GuiProcessCore::new(8080, "llama-server");
        assert_eq!(core.count(), 0);
    }

    #[test]
    fn test_is_running() {
        let core = GuiProcessCore::new(8080, "llama-server");
        assert!(!core.is_running(1));
    }

    #[test]
    fn test_list_all_empty() {
        let core = GuiProcessCore::new(8080, "llama-server");
        assert_eq!(core.list_all().len(), 0);
    }
}
