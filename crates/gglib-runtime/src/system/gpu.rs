//! GPU detection and system memory utilities.
//!
//! This module provides the active system probing for GPU hardware
//! and memory detection, implementing the runtime side of the
//! `SystemProbePort` contract.

use gglib_core::utils::process::bounded::{PROBE_WAIT, run_bounded};

use gglib_core::utils::process::cmd;
use gglib_core::utils::system::{GpuInfo, SystemMemoryInfo};
use sysinfo::System;

use super::vulkan_devices::{vulkan_devices, vulkan_gpu_present};
use crate::llama::vulkan_status;

#[path = "device_memory.rs"]
mod device_memory;

use device_memory::other_gpu_memory_bytes;
pub(super) use device_memory::unified_share;

/// Detect GPU hardware and acceleration software.
pub(crate) fn detect_gpu_info() -> GpuInfo {
    let has_metal = cfg!(target_os = "macos");
    let has_nvidia_gpu = detect_nvidia_hardware();
    let cuda_version = check_cuda();
    let vk = vulkan_status();

    GpuInfo {
        has_nvidia_gpu,
        cuda_version,
        has_metal,
        // The loader is installed with many desktop systems that have no GPU
        // for it to drive, where a Vulkan build would run on the CPU and say
        // nothing; a device must be there too.
        has_vulkan: vulkan_gpu_present(std::env::consts::OS, vk.has_loader, &vulkan_devices()),
        vulkan_headers: vk.has_headers,
        vulkan_glslc: vk.has_glslc,
        vulkan_spirv_headers: vk.has_spirv_headers,
    }
}

/// The GPU a choice between an engine's downloads reads: probed on Linux and
/// Windows, where the choice depends on it, and not on macOS, where there is
/// one build, Metal.
pub(crate) fn gpu_for_downloads() -> GpuInfo {
    if cfg!(any(target_os = "linux", target_os = "windows")) {
        detect_gpu_info()
    } else {
        GpuInfo {
            has_nvidia_gpu: false,
            cuda_version: None,
            has_metal: cfg!(target_os = "macos"),
            has_vulkan: false,
            vulkan_headers: false,
            vulkan_glslc: false,
            vulkan_spirv_headers: false,
        }
    }
}

/// Detect if NVIDIA GPU hardware is present (regardless of CUDA installation).
fn detect_nvidia_hardware() -> bool {
    // Try nvidia-smi (most reliable if NVIDIA drivers are installed)
    if run_bounded(cmd("nvidia-smi").arg("--list-gpus"), PROBE_WAIT)
        .is_ok_and(|o| o.status.success())
    {
        return true;
    }

    // Try lspci on Linux
    #[cfg(target_os = "linux")]
    {
        if run_bounded(&mut cmd("lspci"), PROBE_WAIT).is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .to_lowercase()
                    .contains("nvidia")
        }) {
            return true;
        }
    }

    // Try wmic on Windows
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = run_bounded(
            cmd("wmic").args(["path", "win32_VideoController", "get", "name"]),
            PROBE_WAIT,
        ) && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.to_lowercase().contains("nvidia") {
                return true;
            }
        }
    }

    false
}

/// Check if NVIDIA CUDA toolkit is installed.
pub(crate) fn check_cuda() -> Option<String> {
    // Try nvcc first (CUDA compiler)
    if let Ok(output) = run_bounded(cmd("nvcc").arg("--version"), PROBE_WAIT)
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Extract version from "Cuda compilation tools, release 12.0, V12.0.140"
        if let Some(line) = stdout.lines().find(|l| l.contains("release"))
            && let Some(version) = line.split("release").nth(1)
        {
            let version = version.trim().split(',').next().unwrap_or("").trim();
            if !version.is_empty() {
                return Some(version.to_string());
            }
        }
    }

    None
}

/// Get NVIDIA GPU VRAM in bytes using nvidia-smi.
pub(crate) fn get_nvidia_vram_bytes() -> Option<u64> {
    query_nvidia_memory("memory.total")
}

/// Free NVIDIA VRAM in bytes, right now, using nvidia-smi.
///
/// The sibling of [`get_nvidia_vram_bytes`], and the one the residency
/// decision actually needs: nominal capacity says nothing about whether a
/// second model fits *alongside the one already loaded*, which is the only
/// question worth asking at admission time.
///
/// This is a machine-wide reading, not a gglib-scoped one — another process
/// can take memory between this call and the spawn that follows it. Callers
/// leave a margin for that (see
/// [`RESIDENCY_UTILISATION`](gglib_core::domain::RESIDENCY_UTILISATION)).
pub(crate) fn get_nvidia_free_vram_bytes() -> Option<u64> {
    query_nvidia_memory("memory.free")
}

/// Read one `nvidia-smi --query-gpu` memory field, in bytes.
///
/// Multi-GPU hosts report one line per device; the first is taken, matching
/// what llama-server does by default with no `CUDA_VISIBLE_DEVICES` set.
fn query_nvidia_memory(field: &str) -> Option<u64> {
    let output = run_bounded(
        cmd("nvidia-smi").args([
            &format!("--query-gpu={field}"),
            "--format=csv,noheader,nounits",
        ]),
        PROBE_WAIT,
    )
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // nvidia-smi returns memory in MiB, convert to bytes.
    let mib: u64 = stdout.lines().next()?.trim().parse().ok()?;
    Some(mib * 1024 * 1024)
}

/// Host RAM currently available, in bytes.
///
/// Deliberately `System::new()` + `refresh_memory()` rather than
/// [`System::new_all`]: the latter enumerates every process on the machine,
/// which is many milliseconds of work to answer a question about memory. This
/// is on the admission path (via `free_gpu_memory_bytes` on unified-memory
/// Macs), so the difference matters.
pub(crate) fn get_available_ram_bytes() -> u64 {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.available_memory()
}

/// Get system memory information for model fit calculations.
pub(crate) fn get_system_memory_info() -> SystemMemoryInfo {
    memory_info_for(&detect_gpu_info())
}

/// [`get_system_memory_info`] for the GPU `gpu_info` already found, so a
/// caller that needs both probes the GPU once.
pub(crate) fn memory_info_for(gpu_info: &GpuInfo) -> SystemMemoryInfo {
    let sys = System::new_all();
    let total_ram_bytes = sys.total_memory();

    let (gpu_memory_bytes, is_unified_memory) = if gpu_info.has_metal {
        // Apple Silicon: unified memory architecture.
        (Some(unified_share(total_ram_bytes)), true)
    } else if gpu_info.has_nvidia_gpu {
        // NVIDIA GPU: query VRAM via nvidia-smi, and refuse if it cannot
        // answer. Deliberately *not* falling through to the Vulkan probe: on a
        // hybrid laptop `vulkaninfo`'s first device is frequently the iGPU, so
        // that path would size an 8 GiB card against a share of host RAM —
        // verbatim the outcome `total_device_memory_bytes` refuses to risk. A
        // failing `nvidia-smi` is also the signature of a wedged driver, which
        // is the worst moment to fork another unbounded probe at it.
        (get_nvidia_vram_bytes(), false)
    } else {
        other_gpu_memory_bytes(total_ram_bytes)
    };

    SystemMemoryInfo {
        total_ram_bytes,
        gpu_memory_bytes,
        is_unified_memory,
        has_nvidia_gpu: gpu_info.has_nvidia_gpu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_memory_info() {
        let info = get_system_memory_info();
        // RAM should always be detected and be a reasonable value (> 1GB)
        assert!(info.total_ram_bytes > 1_000_000_000);
    }

    #[test]
    fn test_detect_gpu_info_returns_valid() {
        let info = detect_gpu_info();
        // On any platform, this should return without panicking
        // and has_metal should be true on macOS, false elsewhere
        #[cfg(target_os = "macos")]
        assert!(info.has_metal);
        #[cfg(not(target_os = "macos"))]
        assert!(!info.has_metal);
    }
}
