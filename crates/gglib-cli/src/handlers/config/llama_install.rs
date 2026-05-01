//! llama.cpp source-build installation — CLI surface adapter.
//!
//! Wraps [`run_llama_source_build`] with CLI concerns: dependency checks,
//! the interactive Y/n prompt, and `indicatif` progress rendering.
//! Surface-agnostic build logic lives in `gglib-runtime::llama`.

use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Write};
use std::time::Duration;
use tokio::sync::mpsc;

use gglib_core::paths::{gglib_data_dir, is_prebuilt_binary, llama_cpp_dir, llama_server_path};
use gglib_runtime::llama::{
    Acceleration, BuildEvent, BuildPhase, PrebuiltAvailability, check_dependencies,
    check_disk_space, check_prebuilt_availability, detect_optimal_acceleration_with_diagnostics,
    download_prebuilt_binaries, run_llama_source_build, vulkan_status,
};

fn path_err<T>(r: Result<T, gglib_core::paths::PathError>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Handle the install command.
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
    if server_path.exists() && !force {
        let install_dir = server_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| server_path.display().to_string());
        println!("llama-server is already installed in: {}", install_dir);
        println!("Use --force to rebuild or refresh binaries.");
        return Ok(());
    }

    // Determine installation method
    let should_build = build_from_source
        || !is_prebuilt_binary() // Running from source repo
        || cuda
        || metal
        || vulkan // User specified acceleration flags
        || matches!(
            check_prebuilt_availability(),
            PrebuiltAvailability::NotAvailable { .. }
        );

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

/// CLI-only wrapper for the source-build pipeline.
///
/// Performs dependency checks and the interactive Y/n prompt (CLI concerns), then
/// delegates the actual build work to [`run_llama_source_build`].
async fn build_from_source_impl(cuda: bool, metal: bool, vulkan: bool, force: bool) -> Result<()> {
    // Step 1: Check dependencies.
    check_dependencies()?;
    println!();

    // Step 2: Determine acceleration. The flags that the user passed
    // (--cuda / --metal / --vulkan) imply an explicit opt-in: any
    // missing build dependency must hard-fail with actionable hints.
    // Auto-detect (no flag), by contrast, degrades gracefully to a
    // CPU-only build with a clear warning so a missing optional
    // package doesn't abort `make setup`.
    let explicit_gpu = cuda || metal || vulkan;
    let acceleration = determine_acceleration(cuda, metal, vulkan, explicit_gpu)?;
    println!("Selected acceleration: {}", acceleration.display_name());

    // Step 2b: Vulkan build-readiness pre-flight.
    // Only reached when Vulkan was explicitly requested -- the
    // auto-detect path now disqualifies Vulkan inside
    // `determine_acceleration` and falls back to CPU before reaching
    // here.
    if acceleration == Acceleration::Vulkan {
        let vk = vulkan_status();
        if !vk.ready_for_build() {
            println!();
            println!("\x1b[1;31m❌ Vulkan build requirements not met\x1b[0m");
            println!();
            println!(
                "  Vulkan runtime (loader): {}",
                if vk.has_loader {
                    "✓ found"
                } else {
                    "✗ missing"
                }
            );
            println!(
                "  Vulkan dev headers:      {}",
                if vk.has_headers {
                    "✓ found"
                } else {
                    "✗ missing"
                }
            );
            println!(
                "  SPIR-V compiler (glslc): {}",
                if vk.has_glslc {
                    "✓ found"
                } else {
                    "✗ missing"
                }
            );
            println!(
                "  SPIR-V headers:          {}",
                if vk.has_spirv_headers {
                    "✓ found"
                } else {
                    "✗ missing"
                }
            );
            println!();
            println!("Install the missing components to build with Vulkan:");
            println!();
            for pkg in &vk.missing {
                println!("  {}:", pkg.label());
                for (distro, cmd) in pkg.install_hints() {
                    println!("    {distro:16} {cmd}");
                }
            }
            println!();
            bail!(
                "Missing Vulkan build dependencies: {}",
                vk.missing
                    .iter()
                    .map(|p| p.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
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
    let (tx, rx) = mpsc::channel::<BuildEvent>(64);
    let build = tokio::spawn(run_llama_source_build(
        acceleration,
        llama_dir,
        server_path,
        tx,
    ));
    consume_build_events_cli(rx).await;
    build.await??;

    Ok(())
}

fn determine_acceleration(
    cuda: bool,
    metal: bool,
    vulkan: bool,
    explicit_gpu: bool,
) -> Result<Acceleration> {
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
        // Auto-detect path: degrade to CPU when no GPU is fully
        // buildable. The diagnostic warnings explain *why* (e.g.
        // 'SPIR-V headers (spirv-headers)') so the downgrade isn't
        // silent.
        debug_assert!(!explicit_gpu, "explicit GPU flag should bypass auto-detect");
        let _ = explicit_gpu;
        let (accel, warnings) = detect_optimal_acceleration_with_diagnostics();
        if !warnings.is_empty() {
            println!();
            println!("\x1b[1;33m⚠  GPU acceleration unavailable — falling back to CPU build\x1b[0m");
            println!();
            for msg in &warnings {
                for line in msg.lines() {
                    println!("  {line}");
                }
                println!();
            }
            println!(
                "\x1b[1;33mTip:\x1b[0m re-run \x1b[1mgglib config llama install --vulkan\x1b[0m\n\
                 after installing the components above to build with GPU\n\
                 acceleration. Or pass \x1b[1m--cuda\x1b[0m / \x1b[1m--metal\x1b[0m for a\n\
                 different backend."
            );
            println!();
        }
        Ok(accel)
    }
}

fn print_preflight_info(acceleration: &Acceleration) -> Result<()> {
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
    println!("  3. Compile llama-server (~3-5 minutes)");

    let gglib_dir = path_err(gglib_data_dir())?;
    println!("  4. Install to {}", gglib_dir.join("bin").display());
    println!();

    Ok(())
}

/// Consumes [`BuildEvent`] values from the build pipeline channel and renders
/// them as `indicatif` spinners and progress bars.
///
/// A single `Option<ProgressBar>` tracks the active indicator. Phases are
/// strictly sequential so there is never more than one active bar at a time.
async fn consume_build_events_cli(mut rx: mpsc::Receiver<BuildEvent>) {
    let spinner_style = ProgressStyle::default_spinner()
        .template("{spinner:.green} [{elapsed_precise}] {msg}")
        .expect("valid spinner template");

    let bar_style = ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}",
        )
        .expect("valid bar template")
        .progress_chars("#>-");

    let mut active: Option<ProgressBar> = None;

    while let Some(event) = rx.recv().await {
        match event {
            BuildEvent::PhaseStarted { phase } => {
                // Clean up any previous indicator before starting a new one.
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
                let pb = match phase {
                    BuildPhase::Compile => {
                        // Length unknown until the first Progress event.
                        let pb = ProgressBar::new(0);
                        pb.set_style(bar_style.clone());
                        pb.set_message("Compiling...");
                        pb
                    }
                    BuildPhase::DependencyCheck => {
                        // CLI performs its own dep-check output before the channel
                        // opens, so no indicatif bar is needed here.
                        continue;
                    }
                    _ => {
                        let msg = match phase {
                            BuildPhase::CloneOrUpdateRepo => "Cloning llama.cpp repository...",
                            BuildPhase::Configure => "Configuring with CMake...",
                            BuildPhase::InstallBinaries => "Installing binaries...",
                            _ => unreachable!(),
                        };
                        let pb = ProgressBar::new_spinner();
                        pb.set_style(spinner_style.clone());
                        pb.set_message(msg);
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb
                    }
                };
                active = Some(pb);
            }
            BuildEvent::PhaseCompleted { .. } => {
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
            }
            BuildEvent::Progress { current, total } => {
                if let Some(pb) = &active {
                    pb.set_length(total);
                    pb.set_position(current);
                }
            }
            BuildEvent::Log { message } => {
                if let Some(pb) = &active {
                    pb.println(&message);
                } else {
                    println!("{}", message);
                }
            }
            BuildEvent::Completed {
                version,
                acceleration,
            } => {
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
                println!();
                println!("✓ llama.cpp installed successfully!");
                println!("  Version:       {}", version);
                println!("  Acceleration:  {}", acceleration);
                println!("You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.");
            }
            BuildEvent::Failed { message } => {
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
                eprintln!("✗ Build failed: {}", message);
            }
        }
    }
}
