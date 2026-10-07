//! llama.cpp source-build installation — CLI surface adapter.
//!
//! Wraps [`run_llama_source_build`] with CLI concerns: dependency checks,
//! the interactive Y/n prompt, and `indicatif` progress rendering.
//! Surface-agnostic build logic lives in `gglib-runtime::llama`.

use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::utils::input;
use gglib_core::paths::{gglib_data_dir, is_prebuilt_binary, llama_cpp_dir, llama_server_path};
use gglib_runtime::llama::{
    Acceleration, BuildEvent, BuildPhase, PrebuiltAvailability, check_dependencies,
    check_prebuilt_availability, detect_optimal_acceleration, run_llama_source_build,
    vulkan_status,
};

/// Handle the install command.
///
/// Installation method is determined by context:
/// - `--build` flag: Always build from source
/// - Running from source repo: Build from source (existing behavior)
/// - Pre-built binary + macOS/Windows: Download pre-built binaries
/// - Pre-built binary + Linux: Build from source (CUDA requires compilation)
pub(crate) async fn handle_install(
    cuda: bool,
    metal: bool,
    vulkan: bool,
    force: bool,
    build_from_source: bool,
) -> Result<()> {
    // Check if already installed
    let server_path = llama_server_path()?;
    if server_path.exists() && !force {
        let install_dir = server_path.parent().map_or_else(
            || server_path.display().to_string(),
            |p| p.display().to_string(),
        );
        println!("llama-server is already installed in: {install_dir}");
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
        match super::llama_prebuilt::install().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                println!();
                println!("⚠️  Failed to download pre-built binaries: {e}");
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

    // Step 2: Determine acceleration. Whether the user passed an
    // explicit GPU flag (--cuda / --metal / --vulkan) or relied on
    // auto-detect, missing build dependencies hard-fail with
    // actionable hints. We do **not** silently degrade to a CPU
    // build when a GPU runtime is detected — the user almost
    // certainly wants to fix the missing package and re-run.
    let acceleration = determine_acceleration(cuda, metal, vulkan)?;
    println!("Selected acceleration: {}", acceleration.display_name());

    // Step 2b: Vulkan build-readiness pre-flight.
    // Reached for both explicit `--vulkan` and auto-detected Vulkan;
    // the strict detector already rejects an unbuildable Vulkan, so
    // this is mostly defence-in-depth (catches the `--vulkan` case
    // where the user opts in despite missing deps).
    if acceleration == Acceleration::Vulkan {
        let vk = vulkan_status();
        if !vk.ready_for_build() {
            println!();
            println!("\x1b[1;31m✗ Vulkan build requirements not met\x1b[0m");
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
                    .map(gglib_runtime::llama::MissingPackage::label)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    println!();

    // Step 3: Interactive pre-flight prompt.
    if !force {
        let install_dir = gglib_data_dir()?.join("bin");
        for line in preflight_lines(acceleration, &install_dir) {
            println!("{line}");
        }
        if !input::prompt_confirmation_default_yes("Continue?")? {
            println!("Installation cancelled.");
            return Ok(());
        }
    }

    // Steps 4-7: delegate to the pure streaming core.
    let llama_dir = llama_cpp_dir()?;
    let server_path = llama_server_path()?;
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
        // Auto-detect: strict. If a GPU runtime is detected but
        // the build deps are incomplete, the strict detector
        // returns Err and we propagate it — a missing
        // `spirv-headers` should be surfaced, not silently
        // swapped for a slow CPU build. Step 2b above (when the
        // user passes --vulkan) and `check_dependencies()` in
        // step 1 already cover the explicit-opt-in case with
        // tailored install hints.
        detect_optimal_acceleration()
    }
}

/// What the pre-flight says before it asks whether to continue.
fn preflight_lines(acceleration: Acceleration, install_dir: &Path) -> Vec<String> {
    let name = acceleration.display_name();
    vec![
        "Pre-flight check:".to_owned(),
        "✓ Build dependencies installed".to_owned(),
        format!("✓ Detected: {name}"),
        String::new(),
        "This will:".to_owned(),
        "  1. Clone llama.cpp repository (~150 MB)".to_owned(),
        format!("  2. Configure with CMake ({name} enabled)"),
        "  3. Compile llama-server (~3-5 minutes)".to_owned(),
        format!("  4. Install to {}", install_dir.display()),
        String::new(),
    ]
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
                    println!("{message}");
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
                println!("  Version:       {version}");
                println!("  Acceleration:  {acceleration}");
                println!("You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.");
            }
            BuildEvent::Failed { message } => {
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
                eprintln!("✗ Build failed: {message}");
            }
        }
    }
}

#[cfg(test)]
#[path = "llama_install_tests.rs"]
mod tests;
