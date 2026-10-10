//! `config llama install` and `rebuild`, and the source build — CLI surface
//! adapter.
//!
//! How to install is `llama_method`'s to choose. The source build here wraps
//! [`run_llama_source_build`] with CLI concerns: dependency checks, the
//! interactive Y/n prompt, and progress rendering. Surface-agnostic build
//! logic lives in `gglib-runtime::llama`.

use anyhow::{Result, bail};
use std::path::Path;
use tokio::sync::mpsc;

use super::llama_events::{
    BuildEnding, CLONING, DownloadEnding, built_and_installed, downloaded_and_installed,
    render_build_events, render_install_events,
};
use super::llama_method::{AccelerationFlags, choose_install_method, install_by};
use crate::utils::input;
use gglib_core::domain::RuntimeKind;
use gglib_core::paths::{gglib_data_dir, is_prebuilt_binary, llama_cpp_dir, llama_server_path};
use gglib_runtime::llama::{
    Acceleration, BuildEvent, BuildTool, LlamaProgressEvent, MissingPackage, VulkanStatus,
    build_tool_install_lines, build_tools, check_prebuilt_availability,
    detect_optimal_acceleration, download_prebuilt_binaries, missing_build_tools,
    run_llama_source_build, vulkan_status,
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

    let flags = AccelerationFlags {
        cuda,
        metal,
        vulkan,
    };
    let method = choose_install_method(
        build_from_source,
        flags,
        !is_prebuilt_binary(),
        check_prebuilt_availability,
    );

    install_by(
        &method,
        async || {
            println!("Attempting to download pre-built llama.cpp binaries...");
            install_prebuilt(downloaded_and_installed).await
        },
        async || build_from_source_impl(flags, force, built_and_installed).await,
    )
    .await
}

/// Download and install pre-built llama.cpp binaries, rendering progress.
pub(super) async fn install_prebuilt(ending: DownloadEnding) -> Result<()> {
    let (tx, rx) = mpsc::channel::<LlamaProgressEvent>(64);
    let install = tokio::spawn(download_prebuilt_binaries(tx));
    render_install_events(
        rx,
        RuntimeKind::Llama.label(),
        ending,
        &mut std::io::stdout(),
    )
    .await;
    install.await?
}

/// What the dependency check prints: each build tool as found or missing,
/// and how to install the tools when one is missing.
fn dependency_lines(tools: &[BuildTool]) -> Vec<String> {
    let mut lines = vec!["Checking build dependencies...".to_owned()];
    lines.extend(tools.iter().map(|tool| match &tool.found {
        Some(found) if tool.name == "C++ compiler" => format!("✓ {} {found}", tool.name),
        Some(found) => format!("✓ {} (version {found})", tool.name),
        None => format!("✗ {} not found", tool.name),
    }));

    if !missing_build_tools(tools).is_empty() {
        lines.push(String::new());
        lines.push("Missing dependencies detected. Please install:".to_owned());
        lines.push(String::new());
        lines.extend(build_tool_install_lines());
        lines.push(String::new());
        lines.push("After installing, run 'gglib config llama install' again.".to_owned());
    }
    lines
}

/// CLI-only wrapper for the source-build pipeline.
///
/// Performs dependency checks and the interactive Y/n prompt (CLI concerns), then
/// delegates the actual build work to [`run_llama_source_build`]. `force`
/// skips the prompt: the caller has the user's yes already.
pub(super) async fn build_from_source_impl(
    flags: AccelerationFlags,
    force: bool,
    ending: BuildEnding,
) -> Result<()> {
    // Step 1: Check dependencies.
    let tools = build_tools();
    for line in dependency_lines(&tools) {
        println!("{line}");
    }
    if !missing_build_tools(&tools).is_empty() {
        bail!("Missing required build dependencies");
    }
    println!();

    // Step 2: Determine acceleration. Whether the user passed an
    // explicit GPU flag (--cuda / --metal / --vulkan) or relied on
    // auto-detect, missing build dependencies hard-fail with
    // actionable hints. We do **not** silently degrade to a CPU
    // build when a GPU runtime is detected — the user almost
    // certainly wants to fix the missing package and re-run.
    let acceleration = determine_acceleration(flags)?;
    println!("Selected acceleration: {}", acceleration.display_name());

    // Step 2b: Vulkan build-readiness pre-flight.
    // Reached for both explicit `--vulkan` and auto-detected Vulkan;
    // the strict detector already rejects an unbuildable Vulkan, so
    // this is mostly defence-in-depth (catches the `--vulkan` case
    // where the user opts in despite missing deps).
    if acceleration == Acceleration::Vulkan {
        let vk = vulkan_status();
        if !vk.ready_for_build() {
            for line in vulkan_unready_lines(&vk) {
                println!("{line}");
            }
            let missing: Vec<&str> = vk.missing.iter().map(MissingPackage::label).collect();
            bail!("Missing Vulkan build dependencies: {}", missing.join(", "));
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
    render_build_events(rx, CLONING, ending, &mut std::io::stdout()).await;
    build.await??;

    Ok(())
}

/// What is said of a Vulkan build that cannot start: each of its four
/// requirements as found or missing, then how to install what is missing.
fn vulkan_unready_lines(vk: &VulkanStatus) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "\x1b[1;31m✗ Vulkan build requirements not met\x1b[0m".to_owned(),
        String::new(),
    ];
    lines.extend(
        [
            ("Vulkan runtime (loader):", vk.has_loader),
            ("Vulkan dev headers:", vk.has_headers),
            ("SPIR-V compiler (glslc):", vk.has_glslc),
            ("SPIR-V headers:", vk.has_spirv_headers),
        ]
        .map(|(what, found)| {
            let state = if found { "✓ found" } else { "✗ missing" };
            format!("  {what:<24} {state}")
        }),
    );
    lines.push(String::new());
    lines.push("Install the missing components to build with Vulkan:".to_owned());
    lines.push(String::new());
    for pkg in &vk.missing {
        lines.push(format!("  {}:", pkg.label()));
        for (distro, cmd) in pkg.install_hints() {
            lines.push(format!("    {distro:16} {cmd}"));
        }
    }
    lines.push(String::new());
    lines
}

fn determine_acceleration(flags: AccelerationFlags) -> Result<Acceleration> {
    let AccelerationFlags {
        cuda,
        metal,
        vulkan,
    } = flags;
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
        // swapped for a slow CPU build. Its error names the
        // Vulkan packages that are missing; step 2b above covers
        // the explicit `--vulkan` case with tailored install hints.
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

#[cfg(test)]
#[path = "llama_install_tests.rs"]
mod tests;
