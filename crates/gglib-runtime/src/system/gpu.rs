//! GPU detection and system memory utilities.
//!
//! This module provides the active system probing for GPU hardware
//! and memory detection, implementing the runtime side of the
//! `SystemProbePort` contract.

use gglib_core::utils::process::cmd;
use gglib_core::utils::system::{GpuInfo, SystemMemoryInfo};
use sysinfo::System;

use crate::llama::vulkan_status;

#[path = "device_memory.rs"]
mod device_memory;

use device_memory::{other_gpu_memory_bytes, unified_share};

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
        has_vulkan: vk.has_loader && vulkan_device_present(),
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

/// Whether this machine has a GPU a Vulkan build can run on, as far as the
/// loader alone cannot tell. On Linux, a GPU device: a DRM render node
/// (`/dev/dri/renderD*`, which an AMD, Intel or NVIDIA driver makes),
/// NVIDIA's own device node, or WSL2's GPU (`/dev/dxg`); and a Vulkan driver
/// for it, an ICD manifest, since the loader library is often installed by
/// other packages with no driver at all. Elsewhere the loader comes with a
/// GPU's driver, so it counts.
pub(crate) fn vulkan_device_present() -> bool {
    #[cfg(target_os = "linux")]
    {
        gpu_device_under(std::path::Path::new("/dev"))
            && vulkan_driver_under(&[
                std::path::Path::new("/usr/share/vulkan/icd.d"),
                std::path::Path::new("/etc/vulkan/icd.d"),
            ])
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Whether one of the ICD directories `dirs` holds a Vulkan driver's
/// manifest, a `.json` file.
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "Linux's driver manifests")
)]
fn vulkan_driver_under(dirs: &[&std::path::Path]) -> bool {
    dirs.iter().any(|dir| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        })
    })
}

/// Whether the device directory `dev` holds a GPU's device node: a DRM
/// render node, NVIDIA's, or WSL2's.
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "Linux's device nodes")
)]
fn gpu_device_under(dev: &std::path::Path) -> bool {
    let render_node = std::fs::read_dir(dev.join("dri")).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
    });
    render_node || dev.join("nvidia0").exists() || dev.join("dxg").exists()
}

/// Detect if NVIDIA GPU hardware is present (regardless of CUDA installation).
fn detect_nvidia_hardware() -> bool {
    // Try nvidia-smi (most reliable if NVIDIA drivers are installed)
    if cmd("nvidia-smi")
        .arg("--list-gpus")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return true;
    }

    // Try lspci on Linux
    #[cfg(target_os = "linux")]
    {
        if cmd("lspci").output().is_ok_and(|output| {
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
        if let Ok(output) = cmd("wmic")
            .args(["path", "win32_VideoController", "get", "name"])
            .output()
            && output.status.success()
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
    if let Ok(output) = cmd("nvcc").arg("--version").output()
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
    let output = cmd("nvidia-smi")
        .args([
            &format!("--query-gpu={field}"),
            "--format=csv,noheader,nounits",
        ])
        .output()
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
    let sys = System::new_all();
    let total_ram_bytes = sys.total_memory();
    let gpu_info = detect_gpu_info();

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

#[cfg(test)]
mod device_tests {
    use super::{gpu_device_under, vulkan_driver_under};

    #[test]
    fn a_vulkan_driver_is_an_icd_manifest() {
        let share = tempfile::tempdir().unwrap();
        let etc = tempfile::tempdir().unwrap();
        assert!(
            !vulkan_driver_under(&[share.path(), etc.path()]),
            "no manifests"
        );
        std::fs::write(share.path().join("README"), "").unwrap();
        assert!(
            !vulkan_driver_under(&[share.path(), etc.path()]),
            "not a manifest"
        );
        std::fs::write(etc.path().join("radeon_icd.x86_64.json"), "{}").unwrap();
        assert!(vulkan_driver_under(&[share.path(), etc.path()]));
        assert!(
            !vulkan_driver_under(&[&share.path().join("gone")]),
            "a missing directory"
        );
    }

    #[test]
    fn a_gpu_is_a_render_node_or_nvidias_or_wsl2s_device_node() {
        let dev = tempfile::tempdir().unwrap();
        assert!(!gpu_device_under(dev.path()), "an empty /dev");

        std::fs::create_dir_all(dev.path().join("dri")).unwrap();
        std::fs::write(dev.path().join("dri/card0"), "").unwrap();
        assert!(
            !gpu_device_under(dev.path()),
            "a display card alone renders nothing"
        );

        std::fs::write(dev.path().join("dri/renderD128"), "").unwrap();
        assert!(gpu_device_under(dev.path()));

        let nvidia = tempfile::tempdir().unwrap();
        std::fs::write(nvidia.path().join("nvidia0"), "").unwrap();
        assert!(gpu_device_under(nvidia.path()));

        let wsl = tempfile::tempdir().unwrap();
        std::fs::write(wsl.path().join("dxg"), "").unwrap();
        assert!(gpu_device_under(wsl.path()), "WSL2's GPU");
    }
}
