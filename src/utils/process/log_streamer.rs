//! Server log streaming utilities.
//!
//! This module provides infrastructure for streaming llama-server output
//! to the GUI in real-time.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

/// Maximum number of log lines to keep in the ring buffer per server
const MAX_LOG_LINES: usize = 5000;

/// A single log entry from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerLogEntry {
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
    /// The log line content
    pub line: String,
    /// Server port this log belongs to
    pub port: u16,
}

impl ServerLogEntry {
    /// Create a new log entry with current timestamp
    pub fn new(line: String, port: u16) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Self {
            timestamp,
            line,
            port,
        }
    }
}

/// Ring buffer storing recent log lines for a server
#[derive(Debug, Default)]
pub struct LogBuffer {
    lines: VecDeque<ServerLogEntry>,
}

impl LogBuffer {
    /// Create a new empty log buffer
    pub fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(MAX_LOG_LINES),
        }
    }

    /// Add a log entry, removing oldest if at capacity
    pub fn push(&mut self, entry: ServerLogEntry) {
        if self.lines.len() >= MAX_LOG_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(entry);
    }

    /// Get all log entries
    pub fn get_all(&self) -> Vec<ServerLogEntry> {
        self.lines.iter().cloned().collect()
    }

    /// Get the last N log entries
    pub fn get_last(&self, n: usize) -> Vec<ServerLogEntry> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(skip).cloned().collect()
    }

    /// Clear all log entries
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Number of entries in the buffer
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Manages log buffers and broadcast channels for all running servers
pub struct ServerLogManager {
    /// Log buffers keyed by port
    buffers: RwLock<HashMap<u16, LogBuffer>>,
    /// Broadcast sender for log events (all ports)
    broadcast_tx: broadcast::Sender<ServerLogEntry>,
}

impl ServerLogManager {
    /// Create a new log manager
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);
        Self {
            buffers: RwLock::new(HashMap::new()),
            broadcast_tx,
        }
    }

    /// Add a log line for a server (sync - can be called from std threads)
    pub fn add_log(&self, port: u16, line: &str) {
        let entry = ServerLogEntry::new(line.to_string(), port);

        // Add to buffer
        {
            let mut buffers = self.buffers.write().unwrap();
            let buffer = buffers.entry(port).or_default();
            buffer.push(entry.clone());
        }

        // Broadcast to subscribers (ignore if no receivers)
        let _ = self.broadcast_tx.send(entry);
    }

    /// Get logs for a specific server
    pub fn get_logs(&self, port: u16) -> Vec<ServerLogEntry> {
        let buffers = self.buffers.read().unwrap();
        buffers.get(&port).map(|b| b.get_all()).unwrap_or_default()
    }

    /// Get a broadcast receiver for log events
    pub fn subscribe(&self) -> broadcast::Receiver<ServerLogEntry> {
        self.broadcast_tx.subscribe()
    }

    /// Clear logs for a server
    pub fn clear_logs(&self, port: u16) {
        let mut buffers = self.buffers.write().unwrap();
        buffers.remove(&port);
    }

    /// Initialize a new buffer for a server (call when server starts)
    pub fn init_server(&self, port: u16) {
        let mut buffers = self.buffers.write().unwrap();
        buffers.insert(port, LogBuffer::new());
    }
}

impl Default for ServerLogManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global log manager instance
static LOG_MANAGER: std::sync::OnceLock<Arc<ServerLogManager>> = std::sync::OnceLock::new();

/// Get or initialize the global log manager
pub fn get_log_manager() -> Arc<ServerLogManager> {
    LOG_MANAGER
        .get_or_init(|| Arc::new(ServerLogManager::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_buffer_capacity() {
        let mut buffer = LogBuffer::new();

        // Fill beyond capacity
        for i in 0..MAX_LOG_LINES + 100 {
            buffer.push(ServerLogEntry::new(format!("line {}", i), 8080));
        }

        // Should be capped at MAX_LOG_LINES
        assert_eq!(buffer.len(), MAX_LOG_LINES);

        // First entry should be line 100 (oldest 100 were dropped)
        let logs = buffer.get_all();
        assert!(logs[0].line.contains("100"));
    }

    #[test]
    fn test_log_buffer_get_last() {
        let mut buffer = LogBuffer::new();

        for i in 0..100 {
            buffer.push(ServerLogEntry::new(format!("line {}", i), 8080));
        }

        let last_10 = buffer.get_last(10);
        assert_eq!(last_10.len(), 10);
        assert!(last_10[0].line.contains("90"));
        assert!(last_10[9].line.contains("99"));
    }

    #[test]
    fn test_log_manager() {
        let manager = ServerLogManager::new();

        manager.init_server(8080);
        manager.add_log(8080, "test line");

        let logs = manager.get_logs(8080);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].line, "test line");

        manager.clear_logs(8080);
        let logs = manager.get_logs(8080);
        assert!(logs.is_empty());
    }
}
