use anyhow::Result;
use gglib_core::paths::{is_prebuilt_binary, llama_cli_path, llama_server_path};
use std::io::{self, Write};

use super::download::{
    PrebuiltAvailability, check_prebuilt_availability, download_prebuilt_binaries,
};
use super::install::handle_install;

// Helper to convert PathError to anyhow::Error
fn path_err<T>(r: Result<T, gglib_core::paths::PathError>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Ensure that llama.cpp binaries are installed.
///
/// Checks for the existence of `llama-server` and `llama-cli`.
/// If missing, automatically installs them using the appropriate method:
///
/// - **Source build** (repo detected): Build from source (existing behavior)
/// - **Pre-built binary + macOS/Windows**: Download pre-built binaries (fast)
/// - **Pre-built binary + Linux**: Build from source (CUDA requires compilation)
pub async fn ensure_llama_initialized() -> Result<()> {
    let server_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;

    if server_path.exists() && cli_path.exists() {
        return Ok(());
    }

    println!();
    println!("⚠️  llama.cpp binaries not found.");
    println!("   Server path: {}", server_path.display());
    println!("   CLI path:    {}", cli_path.display());
    println!();

    // Determine installation method based on context
    if is_prebuilt_binary() {
        // Running from a pre-built/installed binary
        ensure_for_prebuilt_binary().await
    } else {
        // Running from source repository (make setup, cargo run, etc.)
        ensure_for_source_build().await
    }
}

/// Installation flow for users running from source repository.
///
/// This preserves the existing behavior: prompt user and build from source.
async fn ensure_for_source_build() -> Result<()> {
    println!("Running from source repository - will build llama.cpp from source.");
    println!();
    print!("Would you like to install llama.cpp now? [Y/n] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("n") {
        anyhow::bail!(
            "llama.cpp is required to run this command. Run 'gglib llama install' manually."
        );
    }

    println!("Building llama.cpp from source (auto-detecting hardware)...");
    println!();

    // Call the install handler - force build from source since we're in source build mode
    handle_install(false, false, false, false, true).await?;

    Ok(())
}

/// Installation flow for users running a pre-built gglib binary.
///
/// Attempts to download pre-built llama.cpp binaries for macOS/Windows.
/// Falls back to building from source for Linux (CUDA requires compilation).
async fn ensure_for_prebuilt_binary() -> Result<()> {
    match check_prebuilt_availability() {
        PrebuiltAvailability::Available { description, .. } => {
            println!(
                "Pre-built llama.cpp binaries are available for {}.",
                description
            );
            println!();
            print!("Would you like to download them now? [Y/n] ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim().eq_ignore_ascii_case("n") {
                anyhow::bail!(
                    "llama.cpp is required to run this command. Run 'gglib llama install' manually."
                );
            }

            // Try downloading pre-built binaries
            match download_prebuilt_binaries().await {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!();
                    println!("⚠️  Failed to download pre-built binaries: {}", e);
                    println!();
                    println!("Falling back to building from source...");
                    println!();

                    // Fall back to building from source
                    handle_install(false, false, false, false, true).await
                }
            }
        }
        PrebuiltAvailability::NotAvailable { reason } => {
            // Linux or unsupported platform - must build from source
            println!("{}", reason);
            println!();
            println!("llama.cpp will be built from source to enable GPU acceleration.");
            println!();

            // Show required build tools
            print_build_requirements();

            print!("Would you like to build llama.cpp now? [Y/n] ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim().eq_ignore_ascii_case("n") {
                anyhow::bail!(
                    "llama.cpp is required to run this command. Run 'gglib llama install' manually."
                );
            }

            println!("Building llama.cpp from source (auto-detecting hardware)...");
            println!();

            handle_install(false, false, false, false, true).await
        }
    }
}

/// Print required build tools for building from source.
fn print_build_requirements() {
    println!("Required build tools:");
    println!("  • git - for cloning the repository");
    println!("  • cmake - for build configuration");
    println!("  • g++ or clang++ - for compilation");
    println!();

    #[cfg(target_os = "linux")]
    {
        println!("On Ubuntu/Debian, install with:");
        println!("  sudo apt install build-essential cmake git");
        println!();
        println!("On Fedora/RHEL, install with:");
        println!("  sudo dnf install gcc-c++ cmake git");
        println!();
    }

    #[cfg(target_os = "macos")]
    {
        println!("On macOS, install with:");
        println!("  xcode-select --install");
        println!("  brew install cmake");
        println!();
    }

    #[cfg(target_os = "windows")]
    {
        println!("On Windows, install:");
        println!("  • Visual Studio Build Tools (with C++ workload)");
        println!("  • CMake (https://cmake.org/download/)");
        println!("  • Git (https://git-scm.com/download/win)");
        println!();
    }
}
