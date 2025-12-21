//! System probe port for dependency and GPU detection.
//!
//! This port abstracts active system probing (command execution, hardware detection)
//! from the core domain. Implementations live in adapters (e.g., gglib-runtime).
//!
//! # Design Notes
//!
//! - Core owns the trait and types (pure)
//! - Runtime owns the implementation (active probing via `Command::new`)
//! - CLI injects the probe via main.rs

use crate::utils::system::{Dependency, GpuInfo, SystemMemoryInfo};
use thiserror::Error;

/// Errors that can occur during system probing.
#[derive(Debug, Error)]
pub enum SystemProbeError {
    /// Failed to execute a command for dependency checking.
    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    /// Failed to parse version output.
    #[error("Version parse failed for {command}: {reason}")]
    VersionParseFailed { command: String, reason: String },

    /// GPU detection failed.
    #[error("GPU detection failed: {0}")]
    GpuDetectionFailed(String),

    /// System memory query failed.
    #[error("Memory query failed: {0}")]
    MemoryQueryFailed(String),
}

/// Result type for system probe operations.
pub type SystemProbeResult<T> = Result<T, SystemProbeError>;

/// Port for probing system dependencies and hardware.
///
/// Implementations of this trait perform active system probing by executing
/// commands, querying hardware, etc. The core domain uses this trait to
/// remain pure and testable.
///
/// # Example
///
/// ```ignore
/// use gglib_core::ports::SystemProbePort;
///
/// fn check_system(probe: &dyn SystemProbePort) {
///     let deps = probe.check_all_dependencies();
///     let gpu = probe.detect_gpu_info();
///     // ...
/// }
/// ```
pub trait SystemProbePort: Send + Sync {
    /// Check all system dependencies and return their status.
    ///
    /// Returns a list of dependencies with their installation status,
    /// version information, and hints for installation.
    fn check_all_dependencies(&self) -> Vec<Dependency>;

    /// Detect GPU hardware and acceleration software.
    ///
    /// Returns information about available GPUs including NVIDIA/CUDA,
    /// AMD/ROCm, and Apple Metal support.
    fn detect_gpu_info(&self) -> GpuInfo;

    /// Get system memory information for model fit calculations.
    ///
    /// Returns total RAM, GPU memory (if available), and platform info
    /// useful for determining which models can run on this system.
    fn get_system_memory_info(&self) -> SystemMemoryInfo;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::system::DependencyStatus;

    /// Mock implementation for testing.
    struct MockSystemProbe {
        deps: Vec<Dependency>,
        gpu: GpuInfo,
        memory: SystemMemoryInfo,
    }

    impl SystemProbePort for MockSystemProbe {
        fn check_all_dependencies(&self) -> Vec<Dependency> {
            self.deps.clone()
        }

        fn detect_gpu_info(&self) -> GpuInfo {
            self.gpu.clone()
        }

        fn get_system_memory_info(&self) -> SystemMemoryInfo {
            self.memory.clone()
        }
    }

    #[test]
    fn test_mock_probe() {
        let probe = MockSystemProbe {
            deps: vec![
                Dependency::required("cargo", "Rust build tool").with_status(
                    DependencyStatus::Present {
                        version: "1.75.0".to_string(),
                    },
                ),
            ],
            gpu: GpuInfo {
                has_nvidia_gpu: false,
                cuda_version: None,
                has_metal: true,
            },
            memory: SystemMemoryInfo {
                total_ram_bytes: 16 * 1024 * 1024 * 1024,
                gpu_memory_bytes: Some(12 * 1024 * 1024 * 1024),
                is_apple_silicon: true,
                has_nvidia_gpu: false,
            },
        };

        let deps = probe.check_all_dependencies();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "cargo");

        let gpu = probe.detect_gpu_info();
        assert!(gpu.has_metal);
        assert!(!gpu.has_nvidia_gpu);

        let mem = probe.get_system_memory_info();
        assert!(mem.is_apple_silicon);
    }
}
