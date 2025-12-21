//! GUI-oriented process lifecycle management.
//!
//! This module provides process spawning, tracking, and management with
//! integrated log streaming and event broadcasting for GUI use cases.
//!
//! Note: This is distinct from the port-aligned `ProcessCore` in `process_core.rs`
//! which implements the `ProcessRunner` port for CLI use cases.

use super::logs::get_log_manager;
use super::ports::{allocate_port, is_port_available};
use super::shutdown::shutdown_child;
use super::types::{RunningProcess, ServerInfo};
use crate::pidfile::{delete_pidfile, write_pidfile};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, warn};

/// GUI-oriented process lifecycle manager.
///
/// Handles spawning, tracking, and killing llama-server processes with
/// integrated log streaming for GUI applications. Uses `u32` model IDs
/// for frontend compatibility.
///
/// For CLI/port-based process management, see `ProcessCore` in `process_core.rs`.
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
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        &mut self,
        model_id: u32,
        model_name: String,
        model_path: &Path,
        context_size: Option<u64>,
        port: Option<u16>,
        jinja: bool,
        reasoning_format: Option<String>,
    ) -> Result<u16> {
        if self.processes.contains_key(&model_id) {
            return Err(anyhow!("Model {} is already running", model_id));
        }

        if !model_path.exists() {
            return Err(anyhow!("Model file not found: {}", model_path.display()));
        }

        let port = self.resolve_port(port)?;
        let mut child =
            self.build_and_spawn_command(model_path, port, context_size, jinja, reasoning_format)?;
        let pid = child
            .id()
            .ok_or_else(|| anyhow!("Failed to get child PID"))?;

        // Write PID file (u32 model_id cast to i64 for pidfile compatibility)
        if let Err(e) = write_pidfile(model_id as i64, pid, port) {
            debug!("Failed to write PID file: {}", e);
        }

        self.spawn_log_readers(&mut child, port);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let info = ServerInfo::new(model_id, model_name, pid, port, now, context_size);
        let running = RunningProcess::new(info, child);
        self.processes.insert(model_id, running);

        Ok(port)
    }

    fn build_and_spawn_command(
        &self,
        model_path: &Path,
        port: u16,
        context_size: Option<u64>,
        jinja: bool,
        reasoning_format: Option<String>,
    ) -> Result<tokio::process::Child> {
        let mut cmd = Command::new(&self.llama_server_path);
        cmd.arg("-m")
            .arg(model_path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--metrics");

        if let Some(ctx) = context_size {
            cmd.arg("-c").arg(ctx.to_string());
        }

        if jinja {
            cmd.arg("--jinja");
        }

        if let Some(format) = reasoning_format {
            cmd.arg("--reasoning-format").arg(format);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        cmd.spawn()
            .map_err(|e| anyhow!("Failed to spawn llama-server: {}", e))
    }

    fn spawn_log_readers(&self, child: &mut tokio::process::Child, port: u16) {
        if let Some(stdout) = child.stdout.take() {
            let log_port = port;
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let log_manager = get_log_manager();
                let mut lines = reader.lines();
                while let Ok(Some(text)) = lines.next_line().await {
                    log_manager.add_log(log_port, &text);
                }
                debug!(port = %log_port, "stdout reader task exiting");
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let log_port = port;
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let log_manager = get_log_manager();
                let mut lines = reader.lines();
                while let Ok(Some(text)) = lines.next_line().await {
                    log_manager.add_log(log_port, &text);
                }
                debug!(port = %log_port, "stderr reader task exiting");
            });
        }
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

    /// Get information about a running process
    pub fn get_info(&self, model_id: u32) -> Option<&ServerInfo> {
        self.processes.get(&model_id).map(|p| &p.info)
    }

    /// Get mutable information about a running process
    pub fn get_info_mut(&mut self, model_id: u32) -> Option<&mut ServerInfo> {
        self.processes.get_mut(&model_id).map(|p| &mut p.info)
    }

    /// List all running processes
    pub fn list_all(&self) -> Vec<&ServerInfo> {
        debug!(process_count = %self.processes.len(), "ProcessCore: list_all called");
        self.processes.values().map(|p| &p.info).collect()
    }

    /// Check if a model is running
    pub fn is_running(&self, model_id: u32) -> bool {
        self.processes.contains_key(&model_id)
    }

    /// Get count of running processes
    pub fn count(&self) -> usize {
        self.processes.len()
    }

    /// Kill all running processes
    pub async fn kill_all(&mut self) {
        let model_ids: Vec<u32> = self.processes.keys().copied().collect();
        for model_id in model_ids {
            let _ = self.kill(model_id).await;
        }
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

// Note: Drop is not async, so we can't use shutdown_child here.
// Caller should explicitly call kill_all() before dropping if graceful shutdown is needed.
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
