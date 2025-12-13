//! Server log sink port for structured log capture.
//!
//! This port abstracts the destination for server logs (stdout/stderr),
//! allowing different implementations for CLI (noop), Tauri (structured storage),
//! and Axum (SSE streaming).

/// Port for appending server log lines to a sink.
///
/// Implementations should be thread-safe and non-blocking where possible.
pub trait ServerLogSinkPort: Send + Sync {
    /// Append a log line from a server process.
    ///
    /// # Arguments
    ///
    /// * `port` - Port the server is listening on (used for grouping logs)
    /// * `stream_type` - Either "stdout" or "stderr"
    /// * `line` - The log line content (without trailing newline)
    fn append(&self, port: u16, stream_type: &str, line: String);
}
