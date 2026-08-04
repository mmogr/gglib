#![doc = include_str!("README.md")]
mod packages;
mod types;

// Re-export pure domain types only - no active probing functions
pub use packages::{LinuxDistro, PackageNames, install_hint, packages_for, parse_os_release};
pub use types::{Dependency, DependencyStatus, GpuInfo, SystemMemoryInfo};
