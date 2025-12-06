//! Command builder and log streaming for llama-server.
//!
//! This module handles building the llama-server command and
//! capturing stdout/stderr output.

use gglib_core::ports::ServerConfig;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use tracing::debug;

/// Build and spawn a llama-server process.
///
/// # Arguments
///
/// * `llama_server_path` - Path to the llama-server binary
/// * `config` - Server configuration
/// * `port` - Allocated port to use
pub fn build_and_spawn(
    llama_server_path: &str,
    config: &ServerConfig,
    port: u16,
) -> anyhow::Result<Child> {
    let mut cmd = Command::new(llama_server_path);
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
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn llama-server: {}", e))?;

    Ok(child)
}

/// Spawn background threads to stream stdout/stderr logs.
///
/// The threads read lines from the process output and log them
/// via tracing. They exit when the streams close.
pub fn spawn_log_readers(child: &mut Child, port: u16) {
    if let Some(stdout) = child.stdout.take() {
        let log_port = port;
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        debug!(port = %log_port, "stdout: {}", text);
                    }
                    Err(e) => {
                        debug!(port = %log_port, error = %e, "Error reading stdout");
                        break;
                    }
                }
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let log_port = port;
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        debug!(port = %log_port, "stderr: {}", text);
                    }
                    Err(e) => {
                        debug!(port = %log_port, error = %e, "Error reading stderr");
                        break;
                    }
                }
            }
        });
    }
}
