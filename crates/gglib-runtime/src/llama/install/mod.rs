//! Installation command for llama.cpp.

use super::build::build_llama_cpp;
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
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::{self, Write};
use std::process::Command;

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
    cpu_only: bool,
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
        || cuda || metal || cpu_only  // User specified acceleration flags
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
    build_from_source_impl(cuda, metal, cpu_only, force).await
}

/// Build llama.cpp from source (the original installation logic)
async fn build_from_source_impl(
    cuda: bool,
    metal: bool,
    cpu_only: bool,
    force: bool,
) -> Result<()> {
    // Step 1: Check dependencies
    check_dependencies()?;
    println!();

    // Step 2: Determine acceleration
    let acceleration = determine_acceleration(cuda, metal, cpu_only)?;
    println!("Selected acceleration: {}", acceleration.display_name());
    println!();

    // Step 3: Pre-flight check
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

    // Step 4: Clone or update repository
    let llama_dir = path_err(llama_cpp_dir())?;
    let (version, commit_sha) = if llama_dir.exists() {
        println!("Using existing llama.cpp repository...");
        get_repo_info(&llama_dir)?
    } else {
        clone_llama_cpp(&llama_dir)?
    };

    // Step 5: Build llama.cpp
    build_llama_cpp(&llama_dir, acceleration)?;

    // Step 6: Install binary
    let server_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;
    install_binary(&llama_dir, "llama-server", &server_path)?;
    install_binary(&llama_dir, "llama-cli", &cli_path)?;

    // Step 7: Save configuration
    let config = BuildConfig::new(version.clone(), commit_sha, acceleration);
    let config_path = path_err(llama_config_path())?;
    config.save(&config_path)?;

    println!();
    println!("✓ llama.cpp installed successfully!");
    println!("  Server: {}", server_path.display());
    println!("  CLI: {}", cli_path.display());
    println!("  Version: {}", version);
    println!("  Acceleration: {}", acceleration.display_name());
    println!();
    println!("You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.");

    Ok(())
}

/// Determine which acceleration to use
fn determine_acceleration(cuda: bool, metal: bool, cpu_only: bool) -> Result<Acceleration> {
    let flags_set = [cuda, metal, cpu_only].iter().filter(|&&x| x).count();

    if flags_set > 1 {
        bail!("Only one acceleration flag can be specified");
    }

    if cpu_only {
        Ok(Acceleration::Cpu)
    } else if metal {
        #[cfg(not(target_os = "macos"))]
        bail!("Metal acceleration is only available on macOS");

        #[cfg(target_os = "macos")]
        Ok(Acceleration::Metal)
    } else if cuda {
        Ok(Acceleration::Cuda)
    } else {
        // Auto-detect
        Ok(detect_optimal_acceleration())
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

/// Clone the llama.cpp repository
fn clone_llama_cpp(llama_dir: &std::path::Path) -> Result<(String, String)> {
    println!("Cloning llama.cpp repository...");
    println!();

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb.set_message("Cloning from GitHub...");

    // Ensure parent directory exists
    if let Some(parent) = llama_dir.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directory")?;
    }

    let status = Command::new("git")
        .args([
            "clone",
            "--depth=1",
            "https://github.com/ggerganov/llama.cpp",
            llama_dir.to_str().unwrap(),
        ])
        .status()
        .context("Failed to run git clone")?;

    pb.finish_and_clear();

    if !status.success() {
        bail!("Failed to clone llama.cpp repository");
    }

    println!("✓ Repository cloned");

    get_repo_info(llama_dir)
}

/// Get version and commit info from repository
fn get_repo_info(llama_dir: &std::path::Path) -> Result<(String, String)> {
    // Get commit SHA
    let output = Command::new("git")
        .args(["-C", llama_dir.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .context("Failed to get commit SHA")?;

    let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Get short version
    let output = Command::new("git")
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
