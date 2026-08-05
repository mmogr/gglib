use anyhow::Result;
use gglib_core::paths::{is_prebuilt_binary, llama_cpp_dir, llama_server_path};
use tokio::sync::mpsc;

use super::build_events::BuildEvent;
use super::detect::detect_optimal_acceleration;
use super::download::{
    PrebuiltAvailability, check_prebuilt_availability, download_prebuilt_binaries,
};
use super::install::run_llama_source_build;
use super::prompt::InstallPrompt;

/// Message used for every "shall I install?" confirmation.
///
/// One constant rather than three near-identical strings: the three branches
/// differ in *what* they are about to do, which is explained by the lines
/// printed before the prompt, not by the prompt itself.
const CONFIRM_INSTALL: &str = "Would you like to install llama.cpp now?";

/// Refusal is not an error condition to explain twice — one message, wherever
/// the user declined.
fn declined() -> anyhow::Error {
    anyhow::anyhow!(
        "llama.cpp is required to run this command. Run 'gglib config llama install' manually."
    )
}

// Helper to convert PathError to anyhow::Error
fn path_err<T>(r: Result<T, gglib_core::paths::PathError>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Ensure that llama.cpp binaries are installed.
///
/// Checks for the existence of `llama-server`.
/// If missing, automatically installs using the appropriate method:
///
/// - **Source build** (repo detected): Build from source (existing behavior)
/// - **Pre-built binary + macOS/Windows**: Download pre-built binaries (fast)
/// - **Pre-built binary + Linux**: Build from source (CUDA requires compilation)
///
/// # Prompting
///
/// Confirmation goes through [`InstallPrompt`] rather than stdin directly, so
/// the caller decides the policy: [`CliPrompt`] asks, [`AutoConfirmPrompt`]
/// proceeds (`gglib up --yes`), and [`NonInteractivePrompt`] refuses rather
/// than installing behind a user's back.
///
/// [`CliPrompt`]: super::prompt::CliPrompt
/// [`AutoConfirmPrompt`]: super::prompt::AutoConfirmPrompt
/// [`NonInteractivePrompt`]: super::prompt::NonInteractivePrompt
pub async fn ensure_llama_initialized(prompt: &dyn InstallPrompt) -> Result<()> {
    let server_path = path_err(llama_server_path())?;

    if server_path.exists() {
        return Ok(());
    }

    println!();
    println!("⚠️  llama.cpp binaries not found.");
    println!("   Server path: {}", server_path.display());
    println!();

    // Determine installation method based on context
    if is_prebuilt_binary() {
        // Running from a pre-built/installed binary
        ensure_for_prebuilt_binary(prompt).await
    } else {
        // Running from source repository (make setup, cargo run, etc.)
        ensure_for_source_build(prompt).await
    }
}

/// Installation flow for users running from source repository.
///
/// This preserves the existing behavior: prompt user and build from source.
async fn ensure_for_source_build(prompt: &dyn InstallPrompt) -> Result<()> {
    println!("Running from source repository - will build llama.cpp from source.");
    print_build_duration_warning();

    if !prompt.confirm(CONFIRM_INSTALL, true)? {
        return Err(declined());
    }

    println!("Building llama.cpp from source (auto-detecting hardware)...");
    println!();

    // Run source build - acceleration is auto-detected
    install_from_source().await?;

    Ok(())
}

/// Installation flow for users running a pre-built gglib binary.
///
/// Attempts to download pre-built llama.cpp binaries for macOS/Windows.
/// Falls back to building from source for Linux (CUDA requires compilation).
async fn ensure_for_prebuilt_binary(prompt: &dyn InstallPrompt) -> Result<()> {
    match check_prebuilt_availability() {
        PrebuiltAvailability::Available { description, .. } => {
            println!("Pre-built llama.cpp binaries are available for {description}.");
            println!();

            if !prompt.confirm("Would you like to download them now?", true)? {
                return Err(declined());
            }

            // Try downloading pre-built binaries
            match download_prebuilt_binaries().await {
                Ok(()) => Ok(()),
                Err(e) => {
                    println!();
                    println!("⚠️  Failed to download pre-built binaries: {e}");
                    println!();
                    println!("Falling back to building from source...");
                    println!();

                    // Fall back to building from source
                    install_from_source().await
                }
            }
        }
        PrebuiltAvailability::NotAvailable { reason } => {
            // Linux or unsupported platform - must build from source
            println!("{reason}");
            println!();
            println!("llama.cpp will be built from source to enable GPU acceleration.");
            print_build_duration_warning();

            // Show required build tools
            print_build_requirements();

            if !prompt.confirm("Would you like to build llama.cpp now?", true)? {
                return Err(declined());
            }

            println!("Building llama.cpp from source (auto-detecting hardware)...");
            println!();

            install_from_source().await
        }
    }
}

/// Runs a source build and streams events as simple text output.
///
/// Used by the ensure flow which handles its own user prompts; indicatif
/// is intentionally omitted here so this remains surface-agnostic.
async fn install_from_source() -> Result<()> {
    let acceleration = detect_optimal_acceleration()?;
    let llama_dir = path_err(llama_cpp_dir())?;
    let server_path = path_err(llama_server_path())?;

    let (tx, mut rx) = mpsc::channel::<BuildEvent>(64);
    let build = tokio::spawn(run_llama_source_build(
        acceleration,
        llama_dir,
        server_path,
        tx,
    ));

    while let Some(event) = rx.recv().await {
        match event {
            BuildEvent::PhaseStarted { phase } => println!("→ {phase:?}"),
            BuildEvent::Log { message } => println!("  {message}"),
            BuildEvent::Progress { current, total } => {
                println!("  [{current}/{total}] Compiling...");
            }
            BuildEvent::PhaseCompleted { .. } => {}
            BuildEvent::Completed { version, .. } => {
                println!("✓ Build complete ({version})");
            }
            BuildEvent::Failed { message } => {
                eprintln!("✗ Build failed: {message}");
            }
        }
    }

    build.await?
}

/// State the cost before asking for consent.
///
/// A source build is tens of minutes of CPU. An auto-confirming caller
/// (`gglib up --yes`) never sees the prompt, so this has to be printed
/// unconditionally rather than folded into the question — otherwise the first
/// thing that command does is silently start a half-hour compile.
fn print_build_duration_warning() {
    println!();
    println!("   This compiles llama.cpp for your hardware and typically takes");
    println!("   15-30 minutes. It happens once; later runs reuse the binaries.");
    println!();
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
