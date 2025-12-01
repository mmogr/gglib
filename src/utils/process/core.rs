//! Core process lifecycle management.
//!
//! This module provides low-level process spawning, tracking, and management
//! functionality that is shared across different process management strategies.

use crate::utils::process::types::{RunningProcess, ServerInfo};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// Core process lifecycle manager
///
/// Handles spawning, tracking, and killing llama-server processes.
/// Does NOT contain any high-level logic like model swapping or health checks.
pub struct ProcessCore {
    /// Running processes keyed by model_id
    processes: HashMap<u32, RunningProcess>,
    /// Base port for allocation
    base_port: u16,
    /// Path to llama-server binary
    llama_server_path: String,
}

impl ProcessCore {
    /// Create a new ProcessCore
    pub fn new(base_port: u16, llama_server_path: impl Into<String>) -> Self {
        Self {
            processes: HashMap::new(),
            base_port,
            llama_server_path: llama_server_path.into(),
        }
    }

    /// Spawn a new llama-server process
    ///
    /// Returns the port number and ServerInfo for the spawned process.
    ///
    /// # Arguments
    /// * `model_id` - Unique model identifier
    /// * `model_name` - Human-readable model name
    /// * `model_path` - Path to the model file
    /// * `context_size` - Optional context window size
    /// * `port` - Optional specific port (None = auto-allocate)
    /// * `jinja` - Enable Jinja templating
    /// * `reasoning_format` - Optional reasoning format for thinking models
    pub fn spawn(
        &mut self,
        model_id: u32,
        model_name: String,
        model_path: &Path,
        context_size: Option<u64>,
        port: Option<u16>,
        jinja: bool,
        reasoning_format: Option<String>,
    ) -> Result<u16> {
        // Check if already running
        if self.processes.contains_key(&model_id) {
            return Err(anyhow!("Model {} is already running", model_id));
        }

        // Check if model file exists
        if !model_path.exists() {
            return Err(anyhow!("Model file not found: {}", model_path.display()));
        }

        // Use specified port or auto-allocate
        let port = match port {
            Some(p) => {
                // Validate port is not privileged
                if p < 1024 {
                    return Err(anyhow!(
                        "Port {} is a privileged port. Please use a port >= 1024.",
                        p
                    ));
                }
                // Check if the specified port is available
                if !Self::is_port_available(p) {
                    return Err(anyhow!(
                        "Port {} is already in use. Please choose a different port.",
                        p
                    ));
                }
                p
            }
            None => self.allocate_port()?,
        };

        // Build llama-server command
        let mut cmd = Command::new(&self.llama_server_path);
        cmd.arg("-m")
            .arg(model_path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string());

        // Add context size if specified
        if let Some(ctx) = context_size {
            cmd.arg("-c").arg(ctx.to_string());
        }

        if jinja {
            cmd.arg("--jinja");
        }

        // Add reasoning format if specified (for thinking/reasoning models)
        if let Some(format) = reasoning_format {
            cmd.arg("--reasoning-format").arg(format);
        }

        // For debugging: log to a file instead of suppressing
        let log_path = std::env::temp_dir().join(format!("llama-server-{}.log", port));
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

        if let Some(file) = log_file {
            debug!(log_path = %log_path.display(), "Logging llama-server output");
            cmd.stdout(file.try_clone().unwrap()).stderr(file);
        } else {
            // Fallback: suppress output if logging fails
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }

        // Spawn the process
        let child = cmd
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn llama-server: {}", e))?;

        let pid = child.id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let info = ServerInfo::new(model_id, model_name, pid, port, now, context_size);
        let running = RunningProcess::new(info, child);

        self.processes.insert(model_id, running);

        Ok(port)
    }

    /// Kill a running process with forced termination (non-blocking)
    pub fn kill(&mut self, model_id: u32) -> Result<()> {
        #[cfg(unix)]
        {
            if let Some(running) = self.processes.remove(&model_id) {
                let pid = running.info.pid;
                debug!(
                    model_id = %model_id,
                    pid = %pid,
                    port = %running.info.port,
                    "Found process to kill"
                );
                use std::process::Command as SysCommand;
                tracing::debug!("Sending SIGKILL to process {}", pid);

                // Spawn kill command without waiting for it
                let _ = SysCommand::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();

                // Drop the child handle immediately - don't try to wait
                // The OS will clean up the zombie process
                drop(running);
                tracing::debug!("SIGKILL sent to process {}, handle dropped", pid);
                Ok(())
            } else {
                Err(anyhow!("Model {} is not running", model_id))
            }
        }

        #[cfg(not(unix))]
        {
            if let Some(mut running) = self.processes.remove(&model_id) {
                let pid = running.info.pid;
                if let Err(e) = running.child.kill() {
                    tracing::warn!("Failed to kill process {}: {}", pid, e);
                }
                let _ = running.child.try_wait();
                Ok(())
            } else {
                Err(anyhow!("Model {} is not running", model_id))
            }
        }
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
        debug!(
            process_count = %self.processes.len(),
            "ProcessCore: list_all called"
        );
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
    pub fn kill_all(&mut self) {
        let model_ids: Vec<u32> = self.processes.keys().copied().collect();
        for model_id in model_ids {
            let _ = self.kill(model_id); // Best effort
        }
    }

    /// Allocate an available port
    /// Check if a port is available by attempting to bind to it
    /// This method binds and immediately drops the listener, which releases the port
    fn is_port_available(port: u16) -> bool {
        use std::net::TcpListener;
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => {
                // Get the actual bound address to ensure it worked
                listener.local_addr().is_ok()
            }
            Err(_) => false,
        }
    }

    fn allocate_port(&self) -> Result<u16> {
        let used_ports: Vec<u16> = self.processes.values().map(|p| p.info.port).collect();

        // Try multiple times with small delays to handle race conditions
        for attempt in 0..3 {
            for offset in 0..100 {
                let port = self.base_port + offset;

                // Skip ports we're already tracking
                if used_ports.contains(&port) {
                    continue;
                }

                // Check if port is actually available
                if Self::is_port_available(port) {
                    debug!(
                        port = %port,
                        attempt = %(attempt + 1),
                        "Allocated available port"
                    );

                    // Double-check availability immediately before returning
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    if Self::is_port_available(port) {
                        return Ok(port);
                    } else {
                        debug!(port = %port, "Port became unavailable, retrying");
                    }
                } else {
                    debug!(port = %port, "Port unavailable on system, skipping");
                }
            }

            // Small delay between attempts
            if attempt < 2 {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }

        Err(anyhow!(
            "No available ports in range {}-{} after 3 attempts",
            self.base_port,
            self.base_port + 99
        ))
    }

    /// Remove dead processes from tracking
    pub fn cleanup_dead(&mut self) -> Vec<u32> {
        debug!(
            process_count = %self.processes.len(),
            "cleanup_dead called"
        );
        let dead: Vec<u32> = self
            .processes
            .iter_mut()
            .filter_map(|(id, running)| {
                // Check if process is still alive
                match running.child.try_wait() {
                    Ok(Some(status)) => {
                        debug!(id = %id, status = ?status, "Process exited");
                        Some(*id) // Process has exited
                    }
                    Ok(None) => {
                        debug!(id = %id, "Process still running");
                        None // Still running
                    }
                    Err(e) => {
                        warn!(id = %id, error = %e, "Error checking process");
                        Some(*id) // Error checking, assume dead
                    }
                }
            })
            .collect();

        // Remove dead processes
        for id in &dead {
            debug!(id = %id, "Removing dead process from map");
            self.processes.remove(id);
        }

        debug!(
            removed_count = %dead.len(),
            remaining_count = %self.processes.len(),
            "cleanup_dead finished"
        );
        dead
    }
}

impl Drop for ProcessCore {
    fn drop(&mut self) {
        self.kill_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_creation() {
        let core = ProcessCore::new(8080, "llama-server");
        assert_eq!(core.count(), 0);
        assert_eq!(core.base_port, 8080);
    }

    #[test]
    fn test_port_allocation() {
        // Use a high port range (49152-65535 is the dynamic/private port range)
        // to minimize conflicts with other services
        let base_port = 59080;
        let mut core = ProcessCore::new(base_port, "llama-server");
        let port1 = core.allocate_port().unwrap();
        // Port should be at or near base_port (may skip if base is in use)
        assert!(port1 >= base_port && port1 < base_port + 100);

        // Simulate a process using the allocated port
        let info = ServerInfo::new(1, "test".to_string(), 1234, port1, 0, None);
        let child = Command::new("echo").spawn().unwrap();
        core.processes.insert(1, RunningProcess::new(info, child));

        let port2 = core.allocate_port().unwrap();
        // Second port should be different and higher than the first
        assert!(port2 > port1);
        assert!(port2 < base_port + 100);
    }

    #[test]
    fn test_is_running() {
        let core = ProcessCore::new(8080, "llama-server");
        assert!(!core.is_running(1));
    }

    #[test]
    fn test_list_all_empty() {
        let core = ProcessCore::new(8080, "llama-server");
        assert_eq!(core.list_all().len(), 0);
    }
}
