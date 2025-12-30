//! Core process lifecycle management.
//!
//! This module provides low-level process spawning, tracking, and management.
//! It operates purely on OS primitives and configuration — no domain logic.

use anyhow::{Result, anyhow};
use gglib_core::ports::{ProcessHandle, ServerConfig};
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Child;
use tracing::debug;

use crate::command;
use crate::pidfile::{delete_pidfile, write_pidfile};
use crate::process::shutdown::shutdown_child;

/// Running process with handle to the child process.
struct RunningProcess {
    handle: ProcessHandle,
    child: Child,
    context_size: Option<u64>,
}

/// Core process lifecycle manager.
///
/// Handles spawning, tracking, and killing llama-server processes.
/// Contains NO high-level logic like model swapping or domain decisions.
pub struct ProcessCore {
    /// Running processes keyed by `model_id`.
    processes: HashMap<i64, RunningProcess>,
    /// Path to llama-server binary.
    llama_server_path: PathBuf,
}

impl ProcessCore {
    /// Create a new `ProcessCore`.
    pub fn new(llama_server_path: impl Into<PathBuf>) -> Self {
        Self {
            processes: HashMap::new(),
            llama_server_path: llama_server_path.into(),
        }
    }

    /// Spawn a new llama-server process from configuration.
    pub async fn spawn(&mut self, config: &ServerConfig) -> Result<ProcessHandle> {
        if self.processes.contains_key(&config.model_id) {
            return Err(anyhow!("Model {} is already running", config.model_id));
        }

        if !config.model_path.exists() {
            return Err(anyhow!(
                "Model file not found: {}",
                config.model_path.display()
            ));
        }

        let port = self.resolve_port(config)?;
        let mut child = command::build_and_spawn(Some(&self.llama_server_path), config, port)?;
        let pid = child
            .id()
            .ok_or_else(|| anyhow!("Failed to get child PID"))?;

        // Write PID file
        if let Err(e) = write_pidfile(config.model_id, pid, port) {
            debug!("Failed to write PID file: {}", e);
            // Non-fatal - continue anyway
        }

        // Wire log capture to the log manager for GUI streaming
        use crate::process::LogManagerSink;
        command::spawn_log_readers(&mut child, port, Some(std::sync::Arc::new(LogManagerSink)));

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let handle = ProcessHandle::new(
            config.model_id,
            config.model_name.clone(),
            Some(pid),
            port,
            now,
        );

        // Note: PID file cleanup on natural exit handled by cleanup_dead() periodic task

        self.processes.insert(
            config.model_id,
            RunningProcess {
                handle: handle.clone(),
                child,
                context_size: config.context_size,
            },
        );

        Ok(handle)
    }

    /// Kill a running process with graceful shutdown.
    pub async fn kill(&mut self, model_id: i64) -> Result<()> {
        let running = self
            .processes
            .remove(&model_id)
            .ok_or_else(|| anyhow!("Model {} is not running", model_id))?;

        let pid = running.handle.pid.unwrap_or(0);
        debug!(model_id = %model_id, pid = %pid, "Stopping process");

        // Use graceful shutdown with SIGTERM → SIGKILL
        let _ = shutdown_child(running.child).await;

        // Remove PID file
        if let Err(e) = delete_pidfile(model_id) {
            debug!("Failed to delete PID file: {}", e);
        }

        Ok(())
    }

    /// Get context size for a running process.
    pub fn get_context_size(&self, model_id: i64) -> Option<u64> {
        self.processes.get(&model_id).and_then(|p| p.context_size)
    }

    /// List all running process handles.
    pub fn list_all(&self) -> Vec<ProcessHandle> {
        self.processes.values().map(|p| p.handle.clone()).collect()
    }

    /// Check if a model is running.
    pub fn is_running(&self, model_id: i64) -> bool {
        self.processes.contains_key(&model_id)
    }

    /// Get count of running processes.
    pub fn count(&self) -> usize {
        self.processes.len()
    }

    /// Resolve port from config or allocate a new one.
    fn resolve_port(&self, config: &ServerConfig) -> Result<u16> {
        match config.port {
            Some(p) if p < 1024 => Err(anyhow!("Port {} is privileged. Use >= 1024.", p)),
            Some(p) if !Self::is_port_available(p) => Err(anyhow!("Port {} is in use.", p)),
            Some(p) => Ok(p),
            None => self.allocate_port(config.base_port),
        }
    }

    fn is_port_available(port: u16) -> bool {
        TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    fn allocate_port(&self, base_port: u16) -> Result<u16> {
        let used: Vec<u16> = self.processes.values().map(|p| p.handle.port).collect();

        for attempt in 0..3 {
            for offset in 0..100 {
                let port = base_port + offset;
                if used.contains(&port) {
                    continue;
                }
                if Self::is_port_available(port) {
                    std::thread::sleep(Duration::from_millis(10));
                    if Self::is_port_available(port) {
                        return Ok(port);
                    }
                }
            }
            if attempt < 2 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        Err(anyhow!(
            "No available ports in range {}-{}",
            base_port,
            base_port + 99
        ))
    }

    /// Remove dead processes from tracking and clean PID files.
    pub async fn cleanup_dead(&mut self) -> Vec<i64> {
        let mut dead = Vec::new();

        for (id, r) in self.processes.iter_mut() {
            match r.child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    dead.push(*id);
                }
                Ok(None) => {}
            }
        }

        for id in &dead {
            self.processes.remove(id);
            // Remove PID file for naturally exited process
            if let Err(e) = delete_pidfile(*id) {
                debug!("Failed to delete PID file for {}: {}", id, e);
            }
        }

        dead
    }
}

// Note: Drop is not async, so we can't use graceful shutdown (shutdown_child) here.
// Drop implementation uses synchronous kill -9 as best-effort cleanup.
impl Drop for ProcessCore {
    fn drop(&mut self) {
        // Best effort: just kill the child handles
        for (_, running) in self.processes.drain() {
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(running.handle.pid.unwrap_or(0).to_string())
                .output();
        }
    }
}
