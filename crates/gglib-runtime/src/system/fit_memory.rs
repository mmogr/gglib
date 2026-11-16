//! What a context is sized against on this machine: the one decision every
//! launch, every plan of one and the endpoint's advertisement read
//! ([`super::fit_memory`]).
//!
//! A pure function of what was detected, so each case is asserted without
//! the hardware it describes.

use gglib_core::utils::system::{GpuInfo, SystemMemoryInfo};

/// The memory a context fit on this machine spends against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMemory {
    /// A GPU's memory as read: a discrete card's VRAM, or the share of RAM
    /// a unified-memory GPU (Apple Silicon, an integrated GPU) addresses.
    Device {
        /// The figure read.
        bytes: u64,
    },
    /// No GPU: the model runs from system RAM, and is sized against the
    /// share of it a unified-memory GPU gets.
    SystemRam {
        /// The machine's total RAM.
        total_bytes: u64,
        /// The share of it the fit spends against.
        budget_bytes: u64,
    },
    /// A GPU whose memory could not be read: no budget, and the fit
    /// refuses.
    Unreadable,
}

impl FitMemory {
    /// The figure the fit spends against, or `None` when there is none.
    #[must_use]
    pub const fn budget_bytes(self) -> Option<u64> {
        match self {
            Self::Device { bytes } => Some(bytes),
            Self::SystemRam { budget_bytes, .. } => Some(budget_bytes),
            Self::Unreadable => None,
        }
    }

    /// The machine's total RAM when the budget is a share of it.
    #[must_use]
    pub const fn system_ram_bytes(self) -> Option<u64> {
        match self {
            Self::SystemRam { total_bytes, .. } => Some(total_bytes),
            Self::Device { .. } | Self::Unreadable => None,
        }
    }

    /// The GPU's memory, when it was read.
    #[must_use]
    pub const fn device_bytes(self) -> Option<u64> {
        match self {
            Self::Device { bytes } => Some(bytes),
            Self::SystemRam { .. } | Self::Unreadable => None,
        }
    }
}

/// Whether `gpu` found a GPU an engine runs on: Metal, an NVIDIA card, or a
/// Vulkan device that is a GPU (`vulkan_devices`), not a software
/// rasteriser.
pub(crate) const fn a_gpu_is_present(gpu: &GpuInfo) -> bool {
    gpu.has_metal || gpu.has_nvidia_gpu || gpu.has_vulkan
}

/// The decision, from whether a GPU is present and the memory read.
pub(crate) const fn fit_memory_of(_gpu_present: bool, mem: &SystemMemoryInfo) -> FitMemory {
    match mem.gpu_memory_bytes {
        Some(bytes) => FitMemory::Device { bytes },
        None => FitMemory::Unreadable,
    }
}

#[cfg(test)]
#[path = "fit_memory_tests.rs"]
mod tests;
