//! System utility types for dependency and environment detection.
//!
//! This module provides pure domain types for system dependencies,
//! GPU information, and memory details. Active system probing is
//! implemented by `DefaultSystemProbe` in `gglib-runtime`.
//!
//! # Architecture Note
//!
//! Core defines types + the `SystemProbePort` trait (in `ports::system_probe`).
//! Runtime implements `DefaultSystemProbe` which performs actual system queries.

mod types;

// Re-export pure domain types only - no active probing functions
pub use types::{Dependency, DependencyStatus, GpuInfo, SystemMemoryInfo};
