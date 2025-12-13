//! Shared GUI backend facade for gglib adapters.
//!
//! This crate provides `GuiBackend`, a platform-agnostic orchestration layer
//! that both Tauri and Axum adapters delegate to. It ensures feature parity
//! and prevents drift between desktop and web UIs.
//!
//! # Architecture
//!
//! ```text
//! Adapters:     gglib-tauri       gglib-axum
//!                    ↓                 ↓
//! Facade:            └── gglib-gui ────┘
//!                        GuiBackend
//!                            ↓
//! Core:                 gglib-core
//! ```
//!
//! # Rules
//!
//! 1. **No adapter dependencies** - Must not depend on tauri, axum, tower, etc.
//! 2. **Pure orchestration** - All deps injected via `GuiDeps`
//! 3. **Trait-based injection** - Uses port traits, not concrete impls
//! 4. **Semantic errors** - Returns `GuiError`, adapters map to their error types

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dependency warnings - these are used transitively
use async_trait as _;
use gglib_hf as _;
use thiserror as _;
use tokio as _;
#[cfg(test)]
use tokio_test as _;

mod backend;
mod deps;
mod error;

mod downloads;
mod mcp;
mod models;
mod proxy;
mod servers;
mod settings;
pub mod types;

// Primary exports
pub use backend::GuiBackend;
pub use deps::GuiDeps;
pub use error::GuiError;
pub use proxy::ProxyOps;

// Re-export operation modules for direct access if needed
pub use downloads::DownloadOps;
pub use mcp::McpOps;
pub use models::ModelOps;
pub use servers::ServerOps;
pub use settings::SettingsOps;

// Re-export commonly used types from gglib-core for convenience
pub use gglib_core::ModelFilterOptions;
pub use gglib_core::download::QueueSnapshot;
