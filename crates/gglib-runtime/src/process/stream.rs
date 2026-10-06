//! Async stream log readers (non-UTF8-safe).
//!
//! llama-server (and other C/C++ tooling) can emit non-UTF8 bytes on stdout/stderr.
//! Using `BufReader::lines()` will terminate the reader task on invalid UTF-8.
//! This module provides byte-based line reading with lossy UTF-8 decoding so
//! log streaming remains robust.

use super::logs::get_log_manager;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tracing::debug;

/// Read `stream` line by line until it closes, handing each line to the log
/// manager under `port`.
pub(crate) fn spawn_stream_reader(
    stream: impl AsyncRead + Unpin + Send + 'static,
    port: u16,
    stream_type: &'static str,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stream);
        let mut buf: Vec<u8> = Vec::with_capacity(1024);

        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    // Trim trailing newline(s)
                    if buf.last() == Some(&b'\n') {
                        buf.pop();
                        if buf.last() == Some(&b'\r') {
                            buf.pop();
                        }
                    }

                    let line = String::from_utf8_lossy(&buf).to_string();
                    debug!(port = %port, %stream_type, "{}: {}", stream_type, line);
                    get_log_manager().add_log(port, &line);
                }
                Err(e) => {
                    debug!(port = %port, %stream_type, error = %e, "log stream reader exiting due to read error");
                    break;
                }
            }
        }

        debug!(port = %port, %stream_type, "log stream reader task exiting");
    });
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
