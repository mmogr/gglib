//! Source-build installation pipeline for llama.cpp.
//!
//! The primary streaming entry point is [`run_llama_source_build`], which emits
//! [`BuildEvent`] values into a `Sender<BuildEvent>` channel. CLI-only concerns
//! (dependency checks, user prompts) remain in [`build_from_source_impl`].
//!
//! ## Consumer table
//!
//! | Consumer | Output                                                             |
//! |----------|--------------------------------------------------------------------|           
//! | CLI      | `indicatif` progress bar (Phase E, via `consume_build_events_cli`) |
//! | Axum     | SSE stream at `POST /api/system/build-llama-from-source`           |
//! | Tauri    | `llama-build-progress` event to WebView                            |
//!
//! ## Threading model
//!
//! [`clone_llama_cpp`] and [`build_llama_cpp`] call `blocking_send` directly in their
//! function bodies and must run via [`tokio::task::spawn_blocking`] from async contexts.
//! [`run_llama_source_build`] handles this wrapping automatically.

use super::build::build_llama_cpp;
use super::build_events::{BuildEvent, BuildPhase};
use super::config::BuildConfig;
use super::deps::check_dependencies;
use super::detect::{Acceleration, detect_optimal_acceleration};
use super::download::{
    PrebuiltAvailability, check_prebuilt_availability, download_prebuilt_binaries,
};
use anyhow::{Context, Result, bail};
use gglib_core::paths::{
    gglib_data_dir, is_prebuilt_binary, llama_cli_path, llama_config_path, llama_cpp_dir,
    llama_server_path,
};
use gglib_core::utils::process::cmd;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use tokio::sync::mpsc;

// Helper to convert PathError to anyhow::Error
fn path_err<T>(r: Result<T, gglib_core::paths::PathError>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Handle the install command
///
/// Installation method is determined by context:
/// - `--build` flag: Always build from source
/// - Running from source repo: Build from source (existing behavior)
/// - Pre-built binary + macOS/Windows: Download pre-built binaries
/// - Pre-built binary + Linux: Build from source (CUDA requires compilation)
pub async fn handle_install(
    cuda: bool,
    metal: bool,
    vulkan: bool,
    force: bool,
    build_from_source: bool,
) -> Result<()> {
    // Check if already installed
    let server_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;
    if server_path.exists() && cli_path.exists() && !force {
        let install_dir = server_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| server_path.display().to_string());
        println!(
            "llama-server and llama-cli are already installed in: {}",
            install_dir
        );
        println!("Use --force to rebuild or refresh binaries.");
        return Ok(());
    }

    // Determine installation method
    let should_build = build_from_source
        || !is_prebuilt_binary()  // Running from source repo
        || cuda || metal || vulkan  // User specified acceleration flags
        || matches!(check_prebuilt_availability(), PrebuiltAvailability::NotAvailable { .. });

    if !should_build {
        // Try downloading pre-built binaries
        println!("Attempting to download pre-built llama.cpp binaries...");
        match download_prebuilt_binaries().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                println!();
                println!("⚠️  Failed to download pre-built binaries: {}", e);
                println!("Falling back to building from source...");
                println!();
            }
        }
    }

    // Build from source
    build_from_source_impl(cuda, metal, vulkan, force).await
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

/// Consumes [`BuildEvent`] values from the build pipeline channel.
///
/// Phase E will replace this stub with full `indicatif` progress-bar rendering.
async fn consume_build_events_cli(mut rx: mpsc::Receiver<BuildEvent>) {
    // Phase E stub: drain events until the indicatif renderer is implemented.
    while rx.recv().await.is_some() {}
}

/// CLI-only wrapper for the source-build pipeline.
///
/// Performs dependency checks and the interactive Y/n prompt (CLI concerns), then
/// delegates the actual build work to [`run_llama_source_build`].
async fn build_from_source_impl(cuda: bool, metal: bool, vulkan: bool, force: bool) -> Result<()> {
    // Step 1: Check dependencies.
    check_dependencies()?;
    println!();

    // Step 2: Determine acceleration.
    let acceleration = determine_acceleration(cuda, metal, vulkan)?;
    println!("Selected acceleration: {}", acceleration.display_name());
    println!();

    // Step 3: Interactive pre-flight prompt.
    if !force {
        print_preflight_info(&acceleration)?;
        print!("Continue? [Y/n]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().eq_ignore_ascii_case("n") {
            println!("Installation cancelled.");
            return Ok(());
        }
    }

    // Steps 4-7: delegate to the pure streaming core.
    let llama_dir = path_err(llama_cpp_dir())?;
    let server_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;
    let (tx, rx) = mpsc::channel::<BuildEvent>(64);
    let build = tokio::spawn(run_llama_source_build(
        acceleration,
        llama_dir,
        server_path,
        cli_path,
        tx,
    ));
    // Phase E will replace this stub with full indicatif rendering.
    consume_build_events_cli(rx).await;
    build.await??;

    println!();
    println!("✓ llama.cpp installed successfully!");
    println!("You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.");

    Ok(())
}

/// Determine which acceleration to use
fn determine_acceleration(cuda: bool, metal: bool, vulkan: bool) -> Result<Acceleration> {
    let flags_set = [cuda, metal, vulkan].iter().filter(|&&x| x).count();

    if flags_set > 1 {
        bail!("Only one acceleration flag can be specified");
    }

    if metal {
        #[cfg(not(target_os = "macos"))]
        bail!("Metal acceleration is only available on macOS");

        #[cfg(target_os = "macos")]
        Ok(Acceleration::Metal)
    } else if cuda {
        Ok(Acceleration::Cuda)
    } else if vulkan {
        Ok(Acceleration::Vulkan)
    } else {
        // Auto-detect
        detect_optimal_acceleration()
    }
}

/// Print pre-flight information
fn print_preflight_info(acceleration: &Acceleration) -> Result<()> {
    use super::deps::check_disk_space;

    println!("Pre-flight check:");
    println!("✓ Build dependencies installed");

    // Check disk space
    if check_disk_space(800)? {
        println!("✓ Disk space available");
    }

    println!("✓ Detected: {}", acceleration.display_name());
    println!();
    println!("This will:");
    println!("  1. Clone llama.cpp repository (~150 MB)");
    println!(
        "  2. Configure with CMake ({} enabled)",
        acceleration.display_name()
    );
    println!("  3. Compile llama-server and llama-cli (~3-5 minutes)");

    let gglib_dir = path_err(gglib_data_dir())?;
    println!("  4. Install to {}", gglib_dir.join("bin").display());
    println!();

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
    println!();
    println!("Installing {} binary...", binary_name);

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

    println!("✓ {} installed to: {}", binary_name, destination.display());

    Ok(())
}
