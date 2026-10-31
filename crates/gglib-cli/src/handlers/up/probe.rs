//! Steps 1-2: what this machine is, and the engines on it.

use std::io::IsTerminal;
use std::sync::Arc;

use anyhow::{Context, Result};
use gglib_app_services::{GpuInfoDto, SetupDeps, SetupOps, SetupStatus};
use gglib_core::domain::format_gib;
use gglib_core::paths::SD_INSTALL_COMMAND;
use gglib_core::utils::system::SystemMemoryInfo;
use gglib_runtime::DefaultSystemProbe;

use super::{require_tty, row, sgr, step};
use crate::bootstrap::CliContext;
use crate::handlers::config::engine::ensure_installed;
use crate::presentation::style::{RESET, SUCCESS};
use gglib_core::domain::RuntimeKind;
use gglib_runtime::engine::{PinStanding, pin_state};

/// Run steps 1 and 2, returning the memory figures the model choice needs.
///
/// `None` means the probe could not read system memory at all — vanishingly
/// rare, but it must not be mistaken for "this machine has no memory", which
/// would silently turn into "nothing fits".
pub(super) async fn run(ctx: &CliContext, yes: bool) -> Result<Option<SystemMemoryInfo>> {
    // One call for hardware, llama.cpp state and the models directory. This is
    // the same status the GUI's setup wizard renders, so the two surfaces
    // cannot drift into disagreeing about the machine they are running on.
    let setup = SetupOps::new(SetupDeps {
        core: ctx.app.clone(),
        system_probe: Arc::new(DefaultSystemProbe::new()),
    });
    let status = setup
        .get_status()
        .await
        .context("Failed to inspect this system")?;

    step(1, "Hardware");
    row("backend", &backend_label(&status.gpu_info), None);
    let memory = report_memory(&status);
    row(
        "models",
        &status.models_directory.path,
        (!status.models_directory.exists).then_some("will be created"),
    );

    step(2, "Engines");
    match status.llama_pin.standing {
        PinStanding::Missing => {
            if !yes {
                require_tty("installing llama.cpp")?;
            }
            ensure_installed(ctx, RuntimeKind::Llama, yes).await?;
            println!("  {}\u{2713}{} llama.cpp ready", sgr(SUCCESS), sgr(RESET));
        }
        // An update is offered, and nobody to answer declines it: what is
        // installed already runs.
        PinStanding::Older => {
            ensure_installed(ctx, RuntimeKind::Llama, yes).await?;
            row("chat", "llama.cpp installed", None);
        }
        PinStanding::AtPin | PinStanding::Newer | PinStanding::Unknown => {
            row("chat", "llama.cpp already installed", None);
        }
    }
    ensure_images(ctx, yes).await;

    Ok(memory)
}

/// Install stable-diffusion.cpp, which draws images, as `make setup` and the
/// GUI wizard's Engines step do. Chat does not need it, so it never stops
/// `up`: declined, failed, or with nobody to ask, `up` says how to install it
/// later and goes on to the model.
async fn ensure_images(ctx: &CliContext, yes: bool) {
    let kind = RuntimeKind::StableDiffusion;
    let standing = match pin_state(kind) {
        Ok(state) => state.standing,
        Err(e) => {
            images_skipped(&format!("{e:#}"));
            return;
        }
    };
    match images_action(standing, yes, std::io::stdin().is_terminal()) {
        ImagesAction::SkipNobodyToAsk => {
            images_skipped("there is no terminal to ask, and --yes was not given");
        }
        ImagesAction::Install => match ensure_installed(ctx, kind, yes).await {
            Ok(()) => println!(
                "  {}\u{2713}{} {} ready",
                sgr(SUCCESS),
                sgr(RESET),
                kind.label()
            ),
            Err(e) => images_skipped(&format!("{e:#}")),
        },
        // As for llama.cpp, a declined or failed update keeps what is there.
        ImagesAction::OfferUpdate => match ensure_installed(ctx, kind, yes).await {
            Ok(()) => row("images", &format!("{} installed", kind.label()), None),
            Err(e) => images_skipped(&format!("{e:#}")),
        },
        ImagesAction::AlreadyThere => {
            row(
                "images",
                &format!("{} already installed", kind.label()),
                None,
            );
        }
    }
}

/// What [`ensure_images`] does about the image runtime.
#[derive(Debug, PartialEq, Eq)]
enum ImagesAction {
    /// Missing, and nobody can be asked: skip rather than prompt into a
    /// closed stdin.
    SkipNobodyToAsk,
    /// Missing: offer it, or install it under `--yes`.
    Install,
    /// Older than the pin: offer the update, which may be declined.
    OfferUpdate,
    /// At or past the pin: nothing to do.
    AlreadyThere,
}

/// The rule [`ensure_images`] applies, given where the install stands,
/// `--yes`, and whether stdin is a terminal.
fn images_action(standing: PinStanding, yes: bool, stdin_is_terminal: bool) -> ImagesAction {
    match standing {
        PinStanding::Missing if !yes && !stdin_is_terminal => ImagesAction::SkipNobodyToAsk,
        PinStanding::Missing => ImagesAction::Install,
        PinStanding::Older => ImagesAction::OfferUpdate,
        PinStanding::AtPin | PinStanding::Newer | PinStanding::Unknown => {
            ImagesAction::AlreadyThere
        }
    }
}

/// Say that images were skipped, why, and how to get them later.
fn images_skipped(reason: &str) {
    for line in images_skipped_lines(reason) {
        println!("{line}");
    }
}

/// The lines [`images_skipped`] prints, as data.
fn images_skipped_lines(reason: &str) -> Vec<String> {
    let product = RuntimeKind::StableDiffusion.label();
    vec![
        format!("  \u{26a0}\u{fe0f}  {product} was not installed: {reason}"),
        format!("     Chat works without it. To draw images later, run '{SD_INSTALL_COMMAND}'."),
    ]
}

/// Print the memory rows and assemble the figure the recommendation sizes
/// against.
///
/// The VRAM row is where this command is most likely to mislead, so it says
/// what it knows. A unified-memory device — Apple Silicon or an integrated GPU
/// — gets its own row rather than being labelled VRAM, because its budget is a
/// share of host RAM. A card whose memory could not be read still reports
/// nothing: falling back to system RAM without saying so would quietly
/// recommend a model far too large for it.
fn report_memory(status: &SetupStatus) -> Option<SystemMemoryInfo> {
    let mem = status.system_memory.as_ref()?;
    let has_gpu =
        status.gpu_info.has_metal || status.gpu_info.has_nvidia || status.gpu_info.has_vulkan;

    match mem.gpu_memory_bytes {
        Some(bytes) if mem.is_unified_memory => {
            row("memory", &format_gib(bytes), Some("unified, usable share"));
        }
        Some(bytes) => {
            row("VRAM", &format_gib(bytes), None);
            row("RAM", &format_gib(mem.total_ram_bytes), None);
        }
        None if has_gpu => {
            row(
                "VRAM",
                "not readable on this GPU",
                Some("sizing against system RAM instead"),
            );
            row("RAM", &format_gib(mem.total_ram_bytes), None);
        }
        None => {
            row(
                "RAM",
                &format_gib(mem.total_ram_bytes),
                Some("no GPU detected"),
            );
        }
    }

    Some(mem.clone())
}

/// Name the acceleration backend from what the GPU probe found.
///
/// An NVIDIA card without the CUDA toolkit is called out rather than reported
/// as plain "NVIDIA": it is the difference between a fast install and one
/// that takes the Vulkan download, or a CPU build.
fn backend_label(gpu: &GpuInfoDto) -> String {
    if gpu.has_metal {
        return "Metal".to_string();
    }
    if gpu.has_nvidia {
        return gpu.cuda_version.as_ref().map_or_else(
            || "NVIDIA GPU, no CUDA toolkit".to_string(),
            |v| format!("CUDA {v}"),
        );
    }
    if gpu.has_vulkan {
        return "Vulkan".to_string();
    }
    "CPU only".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu() -> GpuInfoDto {
        GpuInfoDto {
            has_metal: false,
            has_nvidia: false,
            has_vulkan: false,
            cuda_version: None,
            vulkan_headers_installed: false,
            vulkan_glslc_installed: false,
            vulkan_spirv_headers_installed: false,
        }
    }

    #[test]
    fn metal_outranks_everything_else() {
        let g = GpuInfoDto {
            has_metal: true,
            has_vulkan: true,
            ..gpu()
        };
        assert_eq!(backend_label(&g), "Metal");
    }

    #[test]
    fn cuda_version_is_named_when_present() {
        let g = GpuInfoDto {
            has_nvidia: true,
            cuda_version: Some("12.4".to_string()),
            ..gpu()
        };
        assert_eq!(backend_label(&g), "CUDA 12.4");
    }

    /// The distinction that decides whether the install is a download or a
    /// 30-minute build, so it must not be flattened to "NVIDIA".
    #[test]
    fn an_nvidia_card_without_the_toolkit_says_so() {
        let g = GpuInfoDto {
            has_nvidia: true,
            ..gpu()
        };
        assert_eq!(backend_label(&g), "NVIDIA GPU, no CUDA toolkit");
    }

    #[test]
    fn vulkan_is_reported_when_it_is_all_there_is() {
        let g = GpuInfoDto {
            has_vulkan: true,
            ..gpu()
        };
        assert_eq!(backend_label(&g), "Vulkan");
    }

    #[test]
    fn no_gpu_at_all_is_cpu_only() {
        assert_eq!(backend_label(&gpu()), "CPU only");
    }

    /// A missing image runtime is installed when `--yes` or a terminal can
    /// answer, and skipped only when neither can; one already there is left.
    #[test]
    fn images_are_installed_unless_nobody_can_answer() {
        use ImagesAction::*;
        use PinStanding::*;
        assert_eq!(images_action(Missing, false, false), SkipNobodyToAsk);
        assert_eq!(images_action(Missing, true, false), Install);
        assert_eq!(images_action(Missing, false, true), Install);
        assert_eq!(images_action(Older, false, false), OfferUpdate);
        for standing in [AtPin, Newer, Unknown] {
            assert_eq!(images_action(standing, false, true), AlreadyThere);
        }
    }

    /// A skipped image runtime names why, says chat is unaffected, and gives
    /// the command that installs it later.
    #[test]
    fn skipped_images_say_why_and_how_to_install_later() {
        assert_eq!(
            images_skipped_lines("the download failed"),
            [
                "  \u{26a0}\u{fe0f}  stable-diffusion.cpp was not installed: the download failed",
                "     Chat works without it. To draw images later, run 'gglib config sd install'.",
            ]
        );
    }
}
