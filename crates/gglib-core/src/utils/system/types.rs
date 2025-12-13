//! System dependency and GPU detection types.

use serde::{Deserialize, Serialize};

/// Represents the status of a system dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyStatus {
    /// Dependency is installed and available.
    Present { version: String },
    /// Dependency is missing.
    Missing,
    /// Dependency is optional (not required for basic functionality).
    Optional,
}

/// Information about a system dependency.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Name of the dependency (e.g., "cargo", "node").
    pub name: String,
    /// Current status of the dependency.
    pub status: DependencyStatus,
    /// Description of what this dependency is used for.
    pub description: String,
    /// Whether this dependency is required or optional.
    pub required: bool,
    /// Installation instructions or hints.
    pub install_hint: Option<String>,
}

impl Dependency {
    /// Create a new required dependency.
    pub fn required(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DependencyStatus::Missing,
            description: description.into(),
            required: true,
            install_hint: None,
        }
    }

    /// Create a new optional dependency.
    pub fn optional(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DependencyStatus::Optional,
            description: description.into(),
            required: false,
            install_hint: None,
        }
    }

    /// Set installation hint.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.install_hint = Some(hint.into());
        self
    }

    /// Set the status of this dependency.
    #[must_use]
    pub fn with_status(mut self, status: DependencyStatus) -> Self {
        self.status = status;
        self
    }
}

/// GPU hardware detection result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    /// NVIDIA GPU hardware detected (via nvidia-smi, lspci, etc.).
    pub has_nvidia_gpu: bool,
    /// CUDA toolkit installed and available.
    pub cuda_version: Option<String>,
    /// On macOS (Metal always available).
    pub has_metal: bool,
}

/// System memory information for model fit calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMemoryInfo {
    /// Total system RAM in bytes.
    pub total_ram_bytes: u64,
    /// GPU memory in bytes (VRAM for discrete GPUs, or unified memory portion for Apple Silicon).
    /// None if no GPU detected or memory couldn't be determined.
    pub gpu_memory_bytes: Option<u64>,
    /// Whether the system has Apple Silicon with unified memory.
    pub is_apple_silicon: bool,
    /// Whether the system has an NVIDIA GPU.
    pub has_nvidia_gpu: bool,
}
