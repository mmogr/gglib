//! Command builder and log streaming for llama-server.
//!
//! This module handles building the llama-server command and
//! capturing stdout/stderr output.

use crate::llama::{LlamaServerError, resolve_llama_server};
use crate::process::spawn_stream_reader;
use crate::system::is_truthy_flag;
use gglib_core::ports::{ServerConfig, ServerLogSinkPort};
use gglib_core::utils::process::cmd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Child;
use tracing::{debug, info, warn};

/// Whether the `GGLIB_DISABLE_MTP` environment variable requests that MTP
/// speculative-decoding flags be suppressed.
///
/// Truthy values (case-insensitive): `1`, `true`, `yes`, `on`. Anything else
/// (including unset) leaves MTP enabled.
fn mtp_disabled_via_env() -> bool {
    std::env::var("GGLIB_DISABLE_MTP")
        .ok()
        .is_some_and(|v| is_truthy_flag(&v))
}

/// Whether the `GGLIB_DISABLE_CACHE_REUSE` environment variable requests that
/// `--cache-reuse` be suppressed, even when `config.cache_reuse` is set.
///
/// Mirrors [`mtp_disabled_via_env`] exactly: a global kill switch so
/// `--cache-reuse` (numerically not bit-identical to full recompute — it
/// RoPE-shifts cached keys into new positions) can be A/B tested as a
/// suspect without editing whatever launch profile/script set it, e.g.
/// `GGLIB_DISABLE_CACHE_REUSE=1 gglib proxy --cache-reuse 256`.
fn cache_reuse_disabled_via_env() -> bool {
    std::env::var("GGLIB_DISABLE_CACHE_REUSE")
        .ok()
        .is_some_and(|v| is_truthy_flag(&v))
}

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
                    msg.push_str("\n\nPlease install llama.cpp by running:\n  gglib config llama install");
                    anyhow::anyhow!("{}", msg)
                }
                LlamaServerError::NotExecutable { path } => {
                    anyhow::anyhow!(
                        "llama-server binary exists but is not executable: {}\n\nPlease check file permissions or reinstall with:\n  gglib config llama install",
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

    let cmd = build_command(&validated_path, config, port);

    // Log the full invocation. std::process::Command exposes get_program/get_args,
    // so we log before converting to tokio::process::Command.
    info!(
        "spawning llama-server: {} {}",
        cmd.get_program().to_string_lossy(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Convert to async command and attach piped stdio for log streaming.
    let mut cmd = tokio::process::Command::from(cmd);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn llama-server: {}", e))?;

    Ok(child)
}

/// Build the `llama-server` invocation's argument list from a [`ServerConfig`].
///
/// Pure and side-effect-free (never spawns anything), so cache/MTP/reasoning
/// flag-emission logic can be tested directly against the resulting
/// [`std::process::Command`]'s `get_args()` without needing a real or fake
/// binary on disk.
fn build_command(validated_path: &Path, config: &ServerConfig, port: u16) -> std::process::Command {
    let mut cmd = cmd(validated_path);
    cmd.arg("-m")
        .arg(&config.model_path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--metrics");

    // Serialize concurrent requests onto a single slot.
    //
    // Recent llama.cpp builds default to `--parallel 4` with a *unified* KV
    // cache: the `-c` context tokens become a single pool shared across all 4
    // slots, not a per-request allocation. When a client (e.g. the VS Code LLM
    // Gateway) fires two chat completions concurrently against a large-context
    // model, their combined prompts can exceed that shared pool even though
    // each individually fits — llama-server then aborts BOTH with
    // "Context size has been exceeded", surfacing as an empty response.
    //
    // Forcing a single slot gives every request the full `-c` context
    // exclusively; a second concurrent request queues inside llama-server
    // until the slot frees, which the proxy's streaming keepalive path is
    // built to wait out.
    cmd.arg("--parallel").arg("1");

    // Add context size if specified
    if let Some(ctx) = config.context_size {
        cmd.arg("-c").arg(ctx.to_string());
    }

    // Add GPU layers if specified
    if let Some(layers) = config.gpu_layers {
        cmd.arg("-ngl").arg(layers.to_string());
    }

    // Add jinja if enabled
    if config.jinja {
        cmd.arg("--jinja");
    }

    // Add reasoning format if specified
    if let Some(ref format) = config.reasoning_format {
        cmd.arg("--reasoning-format").arg(format);
    }

    // Add the KV cache disk slot-persistence flag if a slot-save directory is set.
    if let Some(ref slot_path) = config.slot_save_path {
        cmd.arg("--slot-save-path").arg(slot_path);
    }

    // `--cache-ram`: llama-server's own host-RAM prompt cache. Deliberately
    // independent of `--slot-save-path` above — the native RAM cache is
    // useful on its own even when disk persistence is off.
    if let Some(mb) = config.cache_ram_mb {
        cmd.arg("--cache-ram").arg(mb.to_string());
    }
    // else: llama-server's own built-in default (8192 MiB) applies

    // `--cache-reuse`: min chunk size (tokens) for KV-shift cache reuse past
    // the first prefix divergence — helps a follow-up prompt whose earlier
    // messages were edited or summarized, which plain prefix matching can't
    // reuse at all. See `cache_reuse_disabled_via_env` for the kill switch.
    if let Some(n) = config.cache_reuse {
        if cache_reuse_disabled_via_env() {
            info!("cache-reuse suppressed via GGLIB_DISABLE_CACHE_REUSE");
        } else {
            cmd.arg("--cache-reuse").arg(n.to_string());
        }
    }

    // `--cache-type-k` / `--cache-type-v`: KV cache element type. Resolved
    // (default q8_0) by `build_server_config` before this point, so these
    // are set on every launch that goes through the canonical builder.
    if let Some(t) = config.cache_type_k {
        cmd.arg("--cache-type-k").arg(t.as_llama_arg());
    }
    if let Some(t) = config.cache_type_v {
        cmd.arg("--cache-type-v").arg(t.as_llama_arg());
    }

    // Add MTP speculative decoding flags if enabled
    //
    // A global kill switch — the `GGLIB_DISABLE_MTP` environment variable set
    // to a truthy value (`1`, `true`, `yes`, case-insensitive) — forces these
    // flags off even for an MTP-tagged model. This exists so speculative
    // decoding can be A/B tested as a suspect for long-context degenerate
    // generations without editing any per-model config: `GGLIB_DISABLE_MTP=1
    // gglib proxy`.
    if let Some(n) = config.spec_draft_n_max {
        if mtp_disabled_via_env() {
            info!("MTP speculative decoding suppressed via GGLIB_DISABLE_MTP");
        } else {
            cmd.arg("--spec-type").arg("draft-mtp");
            cmd.arg("--spec-draft-n-max").arg(n.to_string());
            if let Some(p) = config.spec_draft_p_min {
                cmd.arg("--spec-draft-p-min").arg(p.to_string());
            }
        }
    }

    // Add inference parameters if specified
    if let Some(ref inference) = config.inference_config {
        for arg in inference.to_cli_args() {
            cmd.arg(arg);
        }
    }

    // Add extra arguments
    for arg in &config.extra_args {
        cmd.arg(arg);
    }

    cmd
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
        spawn_stream_reader(stdout, port, "stdout", log_sink.clone());
    }

    if let Some(stderr) = child.stderr.take() {
        spawn_stream_reader(stderr, port, "stderr", log_sink);
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

    #[test]
    fn is_truthy_flag_recognises_on_values() {
        for v in ["1", "true", "TRUE", " yes ", "On", "  on"] {
            assert!(crate::system::is_truthy_flag(v), "{v:?} should be truthy");
        }
        for v in ["0", "false", "no", "off", "", "2", "disable"] {
            assert!(!crate::system::is_truthy_flag(v), "{v:?} should be falsy");
        }
    }

    /// Minimal `ServerConfig` for `build_command` arg-emission tests — every
    /// cache-related field defaults off so each test only sets what it cares
    /// about.
    fn minimal_config() -> ServerConfig {
        ServerConfig {
            model_id: 1,
            model_name: "test-model".to_string(),
            model_path: PathBuf::from("/tmp/test.gguf"),
            base_port: 9000,
            port: None,
            context_size: None,
            gpu_layers: None,
            jinja: false,
            reasoning_format: None,
            spec_draft_n_max: None,
            spec_draft_p_min: None,
            inference_config: None,
            extra_args: vec![],
            slot_save_path: None,
            cache_ram_mb: None,
            cache_reuse: None,
            cache_type_k: None,
            cache_type_v: None,
        }
    }

    /// Flattened `get_args()` output, for substring/adjacency assertions
    /// without depending on exact positional indices.
    fn args_of(cmd: &std::process::Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn cache_ram_and_cache_reuse_omitted_by_default() {
        let config = minimal_config();
        let cmd = build_command(Path::new("/fake/llama-server"), &config, 5500);
        let args = args_of(&cmd);
        assert!(!args.contains(&"--cache-ram".to_string()));
        assert!(!args.contains(&"--cache-reuse".to_string()));
        assert!(!args.contains(&"--slot-save-path".to_string()));
    }

    #[test]
    fn cache_ram_mb_emits_flag_without_slot_save_path() {
        let config = ServerConfig {
            cache_ram_mb: Some(16384),
            ..minimal_config()
        };
        let cmd = build_command(Path::new("/fake/llama-server"), &config, 5500);
        let args = args_of(&cmd);
        assert!(!args.contains(&"--slot-save-path".to_string()));
        let idx = args
            .iter()
            .position(|a| a == "--cache-ram")
            .expect("--cache-ram should be present");
        assert_eq!(args[idx + 1], "16384");
    }

    #[test]
    fn slot_save_path_without_cache_ram_mb_omits_flag() {
        let config = ServerConfig {
            slot_save_path: Some(PathBuf::from("/tmp/slots")),
            ..minimal_config()
        };
        let cmd = build_command(Path::new("/fake/llama-server"), &config, 5500);
        let args = args_of(&cmd);
        assert!(
            !args.contains(&"--cache-ram".to_string()),
            "--cache-ram should be omitted when cache_ram_mb is None (llama-server default applies)"
        );
    }

    #[test]
    fn cache_ram_mb_overrides_legacy_default_even_with_slot_save_path_set() {
        let config = ServerConfig {
            slot_save_path: Some(PathBuf::from("/tmp/slots")),
            cache_ram_mb: Some(8000),
            ..minimal_config()
        };
        let cmd = build_command(Path::new("/fake/llama-server"), &config, 5500);
        let args = args_of(&cmd);
        let idx = args
            .iter()
            .position(|a| a == "--cache-ram")
            .expect("--cache-ram should be present");
        assert_eq!(args[idx + 1], "8000");
    }

    #[test]
    fn cache_reuse_emits_flag_when_set() {
        let config = ServerConfig {
            cache_reuse: Some(256),
            ..minimal_config()
        };
        let cmd = build_command(Path::new("/fake/llama-server"), &config, 5500);
        let args = args_of(&cmd);
        let idx = args
            .iter()
            .position(|a| a == "--cache-reuse")
            .expect("--cache-reuse should be present");
        assert_eq!(args[idx + 1], "256");
    }

    #[test]
    fn cache_type_k_and_v_omitted_by_default() {
        let config = minimal_config();
        let cmd = build_command(Path::new("/fake/llama-server"), &config, 5500);
        let args = args_of(&cmd);
        assert!(!args.contains(&"--cache-type-k".to_string()));
        assert!(!args.contains(&"--cache-type-v".to_string()));
    }

    #[test]
    fn cache_type_k_and_v_emit_their_llama_arg_names() {
        let config = ServerConfig {
            cache_type_k: Some(gglib_core::cache_config::KvCacheType::Q8_0),
            cache_type_v: Some(gglib_core::cache_config::KvCacheType::F16),
            ..minimal_config()
        };
        let cmd = build_command(Path::new("/fake/llama-server"), &config, 5500);
        let args = args_of(&cmd);
        let k_idx = args
            .iter()
            .position(|a| a == "--cache-type-k")
            .expect("--cache-type-k should be present");
        assert_eq!(args[k_idx + 1], "q8_0");
        let v_idx = args
            .iter()
            .position(|a| a == "--cache-type-v")
            .expect("--cache-type-v should be present");
        assert_eq!(args[v_idx + 1], "f16");
    }

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
            jinja: false,
            reasoning_format: None,
            spec_draft_n_max: None,
            spec_draft_p_min: None,
            inference_config: None,
            extra_args: vec![],
            slot_save_path: None,
            cache_ram_mb: None,
            cache_reuse: None,
            cache_type_k: None,
            cache_type_v: None,
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
