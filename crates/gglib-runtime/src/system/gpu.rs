//! GPU detection and system memory utilities.
//!
//! This module provides the active system probing for GPU hardware
//! and memory detection, implementing the runtime side of the
//! `SystemProbePort` contract.

use gglib_core::utils::system::{GpuInfo, SystemMemoryInfo};
use std::process::Command;
use sysinfo::System;

/// Detect GPU hardware and acceleration software.
pub fn detect_gpu_info() -> GpuInfo {
    let has_metal = cfg!(target_os = "macos");
    let has_nvidia_gpu = detect_nvidia_hardware();
    let cuda_version = check_cuda();

    GpuInfo {
        has_nvidia_gpu,
        cuda_version,
        has_metal,
    }
}

/// Detect if NVIDIA GPU hardware is present (regardless of CUDA installation).
fn detect_nvidia_hardware() -> bool {
    // Try nvidia-smi (most reliable if NVIDIA drivers are installed)
    if Command::new("nvidia-smi")
        .arg("--list-gpus")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    // Try lspci on Linux
    #[cfg(target_os = "linux")]
    {
        if Command::new("lspci")
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .to_lowercase()
                        .contains("nvidia")
            })
            .unwrap_or(false)
        {
            return true;
        }
    }

    // Try wmic on Windows
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("wmic")
            .args(["path", "win32_VideoController", "get", "name"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.to_lowercase().contains("nvidia") {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if NVIDIA CUDA toolkit is installed.
pub fn check_cuda() -> Option<String> {
    // Try nvcc first (CUDA compiler)
    if let Ok(output) = Command::new("nvcc").arg("--version").output()
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
pub fn get_nvidia_vram_bytes() -> Option<u64> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // nvidia-smi returns memory in MiB, convert to bytes
    // If multiple GPUs, take the first one
    let mib: u64 = stdout.lines().next()?.trim().parse().ok()?;
    Some(mib * 1024 * 1024)
}

/// Get system memory information for model fit calculations.
pub fn get_system_memory_info() -> SystemMemoryInfo {
    let sys = System::new_all();
    let total_ram_bytes = sys.total_memory();
    let gpu_info = detect_gpu_info();

    let (gpu_memory_bytes, is_apple_silicon) = if gpu_info.has_metal {
        // Apple Silicon: unified memory architecture
        // Approximately 75% of total RAM is available for GPU use
        let gpu_available = (total_ram_bytes as f64 * 0.75) as u64;
        (Some(gpu_available), true)
    } else if gpu_info.has_nvidia_gpu {
        // NVIDIA GPU: query VRAM via nvidia-smi
        let vram = get_nvidia_vram_bytes();
        (vram, false)
    } else {
        // No GPU acceleration available, will use RAM
        (None, false)
    };

    SystemMemoryInfo {
        total_ram_bytes,
        gpu_memory_bytes,
        is_apple_silicon,
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
