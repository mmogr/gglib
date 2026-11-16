#![doc = include_str!("README.md")]
mod needs;
mod packages;
mod report;
mod types;

// Re-export pure domain types only - no active probing functions
pub use needs::{HostOs, NEEDS, Need, Probe, Purpose, Source, need, needs_on};
pub use packages::{LinuxDistro, PackageNames, install_hint, packages_for, parse_os_release};
pub use report::{
    BuildOrigin, DependencyGroup, DependencyReport, EngineInstall, Found, dependency_report,
    hint_for,
};
pub use types::{Dependency, DependencyStatus, GpuInfo, SystemMemoryInfo};
