//! Steps 1-2: what this machine is, and llama.cpp on it.

use std::sync::Arc;

use anyhow::{Context, Result};
use gglib_app_services::{GpuInfoDto, SetupDeps, SetupOps, SetupStatus};
use gglib_core::domain::format_gib;
use gglib_core::utils::system::SystemMemoryInfo;
use gglib_runtime::DefaultSystemProbe;
use gglib_runtime::llama::{AutoConfirmPrompt, CliPrompt, InstallPrompt, ensure_llama_initialized};

use super::{require_tty, row, sgr, step};
use crate::bootstrap::CliContext;
use crate::presentation::style::{RESET, SUCCESS};

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

    step(2, "llama.cpp");
    if status.llama_installed {
        row("binaries", "already installed", None);
    } else {
        let prompt: Box<dyn InstallPrompt> = if yes {
            Box::new(AutoConfirmPrompt)
        } else {
            require_tty("installing llama.cpp")?;
            Box::new(CliPrompt::new())
        };
        ensure_llama_initialized(prompt.as_ref()).await?;
        println!("  {}\u{2713}{} llama.cpp ready", sgr(SUCCESS), sgr(RESET));
    }

    Ok(memory)
}

/// Print the memory rows and assemble the figure the recommendation sizes
/// against.
///
/// The VRAM row is where this command is most likely to mislead, so it says
/// what it knows: gglib reads VRAM for Metal and NVIDIA only, and every
/// Vulkan-only card reports nothing. Falling back to system RAM without saying
/// so would quietly recommend a model far too large for an AMD or Intel GPU.
fn report_memory(status: &SetupStatus) -> Option<SystemMemoryInfo> {
    let mem = status.system_memory.as_ref()?;
    let has_gpu =
        status.gpu_info.has_metal || status.gpu_info.has_nvidia || status.gpu_info.has_vulkan;

    match mem.gpu_memory_bytes {
        Some(bytes) if mem.is_apple_silicon => {
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

    Some(SystemMemoryInfo {
        total_ram_bytes: mem.total_ram_bytes,
        gpu_memory_bytes: mem.gpu_memory_bytes,
        is_apple_silicon: mem.is_apple_silicon,
        has_nvidia_gpu: status.gpu_info.has_nvidia,
    })
}

/// Name the acceleration backend from what the GPU probe found.
///
/// An NVIDIA card without the CUDA toolkit is called out rather than reported
/// as plain "NVIDIA": it is the difference between a fast install and a build
/// that silently falls back to CPU.
fn backend_label(gpu: &GpuInfoDto) -> String {
    if gpu.has_metal {
        return "Metal".to_string();
    }
    if gpu.has_nvidia {
        return match &gpu.cuda_version {
            Some(v) => format!("CUDA {v}"),
            None => "NVIDIA GPU, no CUDA toolkit".to_string(),
        };
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
}
