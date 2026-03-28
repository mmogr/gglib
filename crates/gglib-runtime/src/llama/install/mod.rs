//! Source-build installation pipeline for llama.cpp.
//!
//! The primary streaming entry point is [`run_llama_source_build`], which emits
//! [`BuildEvent`] values into a `Sender<BuildEvent>` channel. CLI surface concerns
//! (dependency checks, user prompts, progress rendering) live in
//! `gglib-cli::handlers::llama_install`.
//!
//! ## Consumer table
//!
//! | Consumer | Crate        | Output                                                          |
//! |----------|--------------|-----------------------------------------------------------------|
//! | CLI      | `gglib-cli`  | `indicatif` spinner + progress bar in `handlers::llama_install` |
//! | Axum     | `gglib-axum` | SSE stream at `POST /api/system/build-llama-from-source`        |
//! | Tauri    | `gglib-tauri`| `llama-build-progress` event to WebView                         |
//!
//! ## Threading model
//!
//! [`clone_llama_cpp`] and [`build_llama_cpp`] call `blocking_send` directly in their
//! function bodies and must run via [`tokio::task::spawn_blocking`] from async contexts.
//! [`run_llama_source_build`] handles this wrapping automatically.

use super::build::build_llama_cpp;
use super::build_events::{BuildEvent, BuildPhase};
use super::config::BuildConfig;
use super::detect::Acceleration;
use anyhow::{Context, Result, bail};
use gglib_core::paths::llama_config_path;
use gglib_core::utils::process::cmd;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use tokio::sync::mpsc;

// Helper to convert PathError to anyhow::Error
fn path_err<T>(r: Result<T, gglib_core::paths::PathError>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Core streaming build pipeline for llama.cpp from source.
///
/// Clones or reuses the repository, configures, compiles, installs binaries, and saves
/// the build configuration. All progress is emitted as [`BuildEvent`] values on `tx`.
///
/// This function has no CLI concerns (no user prompts, no dependency checks). Callers
/// must perform pre-flight validation before calling this.
///
/// # Threading
///
/// Blocking subprocess work ([`clone_llama_cpp`], [`build_llama_cpp`]) runs inside
/// [`tokio::task::spawn_blocking`] so the Tokio executor is never blocked and
/// `blocking_send` calls are always on OS threads.
pub async fn run_llama_source_build(
    acceleration: Acceleration,
    llama_dir: PathBuf,
    server_path: PathBuf,
    cli_path: PathBuf,
    tx: mpsc::Sender<BuildEvent>,
) -> Result<()> {
    // Step 1: Clone or reuse repository.
    let (version, commit_sha) = if llama_dir.exists() {
        let _ = tx
            .send(BuildEvent::Log {
                message: "Using existing llama.cpp repository.".to_string(),
            })
            .await;
        get_repo_info(&llama_dir)?
    } else {
        let tx_clone = tx.clone();
        let dir = llama_dir.clone();
        tokio::task::spawn_blocking(move || clone_llama_cpp(&dir, &tx_clone)).await??
    };

    // Step 2: Configure and compile.
    {
        let tx_clone = tx.clone();
        let dir = llama_dir.clone();
        tokio::task::spawn_blocking(move || build_llama_cpp(&dir, acceleration, &tx_clone))
            .await??;
    }

    // Step 3: Install binaries.
    {
        let tx_clone = tx.clone();
        let dir = llama_dir.clone();
        let sp = server_path.clone();
        let cp = cli_path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let _ = tx_clone.blocking_send(BuildEvent::PhaseStarted {
                phase: BuildPhase::InstallBinaries,
            });
            install_binary(&dir, "llama-server", &sp)?;
            install_binary(&dir, "llama-cli", &cp)?;
            let _ = tx_clone.blocking_send(BuildEvent::PhaseCompleted {
                phase: BuildPhase::InstallBinaries,
            });
            Ok(())
        })
        .await??;
    }

    // Step 4: Persist build configuration.
    let config = BuildConfig::new(version.clone(), commit_sha, acceleration);
    let config_path = path_err(llama_config_path())?;
    config.save(&config_path)?;

    // Step 5: Signal successful completion.
    let _ = tx
        .send(BuildEvent::Completed {
            version,
            acceleration: acceleration.display_name().to_string(),
        })
        .await;

    Ok(())
}

/// Clone the llama.cpp repository, routing subprocess output through `tx`.
///
/// Git progress lines containing `\r` (animated carriage-return output) are filtered
/// out to avoid corrupting SSE streams. Only clean, newline-terminated informational
/// lines are emitted as [`BuildEvent::Log`].
fn clone_llama_cpp(llama_dir: &Path, tx: &mpsc::Sender<BuildEvent>) -> Result<(String, String)> {
    let _ = tx.blocking_send(BuildEvent::PhaseStarted {
        phase: BuildPhase::CloneOrUpdateRepo,
    });

    if let Some(parent) = llama_dir.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directory")?;
    }

    let mut child = cmd("git")
        .args([
            "clone",
            "--depth=1",
            "https://github.com/ggerganov/llama.cpp",
            llama_dir.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to run git clone")?;

    let stderr = child.stderr.take().unwrap();

    // Git writes all progress to stderr. Read on an OS thread (blocking I/O).
    // Carriage-return progress lines (e.g. "Receiving objects: 45%\r") are
    // filtered: BufRead::lines() keeps \r as trailing content; any line
    // containing \r is dropped to avoid corrupting SSE streams.
    let tx_reader = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() || line.contains('\r') {
                continue;
            }
            if tx_reader
                .blocking_send(BuildEvent::Log { message: line })
                .is_err()
            {
                break;
            }
        }
    });

    let status = child.wait().context("Failed to wait for git clone")?;
    if !status.success() {
        bail!("Failed to clone llama.cpp repository");
    }

    let _ = tx.blocking_send(BuildEvent::PhaseCompleted {
        phase: BuildPhase::CloneOrUpdateRepo,
    });

    get_repo_info(llama_dir)
}

/// Get version and commit info from repository
fn get_repo_info(llama_dir: &std::path::Path) -> Result<(String, String)> {
    // Get commit SHA
    let output = cmd("git")
        .args(["-C", llama_dir.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .context("Failed to get commit SHA")?;

    let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Get short version
    let output = cmd("git")
        .args([
            "-C",
            llama_dir.to_str().unwrap(),
            "rev-parse",
            "--short",
            "HEAD",
        ])
        .output()
        .context("Failed to get short commit hash")?;

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok((version, commit_sha))
}

/// Install the requested llama.cpp binary from the build directory into gglib's bin folder.
pub(super) fn install_binary(
    llama_dir: &std::path::Path,
    binary_name: &str,
    destination: &std::path::Path,
) -> Result<()> {
    let binary_src_root = llama_dir.join("build");

    #[cfg(target_os = "windows")]
    let relative_binary = format!("bin\\Release\\{}.exe", binary_name);

    #[cfg(not(target_os = "windows"))]
    let relative_binary = format!("bin/{}", binary_name);

    let binary_src = binary_src_root.join(relative_binary);

    if !binary_src.exists() {
        bail!("Built binary not found at: {}", binary_src.display());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).context("Failed to create bin directory")?;
    }

    fs::copy(&binary_src, destination).context("Failed to copy binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(destination)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(destination, perms)?;
    }

    Ok(())
}
