#![doc = include_str!("README.md")]
mod types;

// Re-export pure domain types only - no active probing functions
pub use types::{Dependency, DependencyStatus, GpuInfo, SystemMemoryInfo};
