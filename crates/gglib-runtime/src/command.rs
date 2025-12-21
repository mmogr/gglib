//! Command builder and log streaming for llama-server.
//!
//! This module handles building the llama-server command and
//! capturing stdout/stderr output.

use crate::llama::{LlamaServerError, resolve_llama_server};
use gglib_core::ports::{ServerConfig, ServerLogSinkPort};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{debug, warn};

/// Select the llama-server path to use.
///
/// This function implements the "bootstrap path wins" rule:
/// 1. If a valid bootstrap path is provided, use it (authoritative)
/// 2. Otherwise, fall back to internal resolution with a warning
///
/// # Arguments
///
/// * `bootstrap_path` - Path provided by bootstrap (from resolved paths)
///
/// # Returns
///
/// The validated path to the llama-server binary.
///
/// # Errors
///
/// Returns an error if neither the bootstrap path nor the fallback resolver
/// can locate a valid llama-server binary.
fn select_llama_path(bootstrap_path: Option<&Path>) -> Result<PathBuf, LlamaServerError> {
    if let Some(path) = bootstrap_path {
        if path.as_os_str().is_empty() {
            warn!("Bootstrap provided empty llama-server path, falling back to resolver");
        } else if path.exists() {
            debug!("Using llama-server from bootstrap: {}", path.display());
            return Ok(path.to_path_buf());
        } else {
            warn!(
                "Bootstrap path does not exist: {}, falling back to resolver",
                path.display()
            );
        }
    }

    // Fallback to internal resolution
    warn!("Bootstrap path invalid/empty, falling back to internal resolver");
    resolve_llama_server()
}

/// Build and spawn a llama-server process.
///
/// This function:
/// 1. Selects the llama-server binary path (bootstrap path wins)
/// 2. Builds the command with all required arguments
/// 3. Spawns the process
///
/// # Arguments
///
/// * `llama_server_path` - Path to the llama-server binary from bootstrap
/// * `config` - Server configuration
/// * `port` - Allocated port to use
///
/// # Errors
///
/// Returns an error if:
/// - The llama-server binary is not found, not executable, or inaccessible
/// - The process fails to spawn for other reasons
pub fn build_and_spawn(
    llama_server_path: Option<&Path>,
    config: &ServerConfig,
    port: u16,
) -> anyhow::Result<Child> {
    // Select the binary path using bootstrap-path-wins rule
    let validated_path = select_llama_path(llama_server_path)
        .map_err(|e| {
            // Convert LlamaServerError to anyhow with full context
            match e {
                LlamaServerError::NotFound { path, legacy_path } => {
                    let mut msg = format!("llama-server binary not found at: {}", path.display());
                    if let Some(legacy) = legacy_path {
                        msg.push_str(&format!(
                            "\n\nFound an older installation at: {}\nConsider moving or symlinking it to the new location.",
                            legacy.display()
                        ));
                    }
                    msg.push_str("\n\nPlease install llama.cpp by running:\n  gglib llama install");
                    anyhow::anyhow!("{}", msg)
                }
                LlamaServerError::NotExecutable { path } => {
                    anyhow::anyhow!(
                        "llama-server binary exists but is not executable: {}\n\nPlease check file permissions or reinstall with:\n  gglib llama install",
                        path.display()
                    )
                }
                LlamaServerError::PermissionDenied { path } => {
                    anyhow::anyhow!(
                        "Permission denied accessing llama-server binary: {}\n\nPlease check file permissions.",
                        path.display()
                    )
                }
                LlamaServerError::PathResolution(msg) => {
                    anyhow::anyhow!("Failed to resolve llama-server path: {}", msg)
                }
            }
        })?;

    let mut cmd = Command::new(validated_path);
    cmd.arg("-m")
        .arg(&config.model_path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--metrics");

    // Add context size if specified
    if let Some(ctx) = config.context_size {
        cmd.arg("-c").arg(ctx.to_string());
    }

    // Add GPU layers if specified
    if let Some(layers) = config.gpu_layers {
        cmd.arg("-ngl").arg(layers.to_string());
    }

    // Add extra arguments
    for arg in &config.extra_args {
        cmd.arg(arg);
    }

    // Use piped stdio for log streaming
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn llama-server: {}", e))?;

    Ok(child)
}

/// Spawn background tasks to stream stdout/stderr logs asynchronously.
///
/// The tasks read lines from the process output and log them
/// via tracing. If a log sink is provided, lines are also forwarded there.
/// They exit when the streams close.
pub fn spawn_log_readers(
    child: &mut Child,
    port: u16,
    log_sink: Option<Arc<dyn ServerLogSinkPort>>,
) {
    if let Some(stdout) = child.stdout.take() {
        let log_port = port;
        let sink = log_sink.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(text)) = lines.next_line().await {
                debug!(port = %log_port, "stdout: {}", text);
                if let Some(ref s) = sink {
                    s.append(log_port, "stdout", text);
                }
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let log_port = port;
        let sink = log_sink;
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(text)) = lines.next_line().await {
                debug!(port = %log_port, "stderr: {}", text);
                if let Some(ref s) = sink {
                    s.append(log_port, "stderr", text);
                }
            }
        });
    }
}

/// A no-op log sink that discards all log lines.
///
/// Useful for CLI usage where structured log capture is not needed.
#[derive(Debug, Clone, Default)]
pub struct NoopLogSink;

impl ServerLogSinkPort for NoopLogSink {
    fn append(&self, _port: u16, _stream_type: &str, _line: String) {
        // Intentionally empty - logs are already going to tracing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    /// Test that a valid bootstrap path is used directly.
    #[test]
    #[cfg(unix)]
    fn test_select_llama_path_uses_valid_bootstrap_path() {
        let temp_dir = TempDir::new().unwrap();
        let binary_path = temp_dir.path().join("llama-server");

        // Create a fake binary
        fs::write(&binary_path, "#!/bin/sh\necho test").unwrap();
        fs::set_permissions(&binary_path, fs::Permissions::from_mode(0o755)).unwrap();

        let result = select_llama_path(Some(&binary_path));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), binary_path);
    }

    /// Test that None bootstrap path triggers fallback.
    #[test]
    fn test_select_llama_path_none_triggers_fallback() {
        let result = select_llama_path(None);
        // Fallback will either succeed (if llama-server is installed) or fail
        // We just verify the function doesn't panic and returns a Result
        let _ = result;
    }

    /// Test that an invalid bootstrap path triggers fallback.
    #[test]
    fn test_select_llama_path_invalid_triggers_fallback() {
        let nonexistent = PathBuf::from("/nonexistent/path/llama-server");
        let result = select_llama_path(Some(&nonexistent));
        // Should attempt fallback (which may succeed or fail)
        let _ = result;
    }

    /// Test that build_and_spawn prefers the injected path when present.
    #[tokio::test]
    #[cfg(unix)]
    async fn test_build_and_spawn_prefers_bootstrap_path() {
        let temp_dir = TempDir::new().unwrap();
        let binary_path = temp_dir.path().join("llama-server");

        // Create a fake binary that exits immediately
        fs::write(&binary_path, "#!/bin/sh\nexit 0").unwrap();
        fs::set_permissions(&binary_path, fs::Permissions::from_mode(0o755)).unwrap();

        let config = ServerConfig {
            model_id: 1,
            model_name: "test-model".to_string(),
            model_path: PathBuf::from("/tmp/test.gguf"),
            base_port: 9000,
            port: Some(8080),
            context_size: None,
            gpu_layers: None,
            extra_args: vec![],
        };

        // Should use the bootstrap path (will spawn then immediately exit)
        let result = build_and_spawn(Some(&binary_path), &config, 8080);

        // We expect this to succeed in spawning (even if the process exits immediately)
        assert!(
            result.is_ok(),
            "build_and_spawn should succeed with valid bootstrap path"
        );
    }
}
