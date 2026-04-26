#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dependency warnings - these are used transitively
use async_trait as _;
use gglib_hf as _;
use thiserror as _;
use tokio as _;
#[cfg(test)]
use tokio_test as _;

mod error;

mod downloads;
mod mcp;
mod models;
mod proxy;
mod servers;
mod settings;
pub mod setup;
pub mod types;

// Primary exports
pub use error::GuiError;

// Domain ops + their Deps
pub use downloads::{DownloadDeps, DownloadOps};
pub use mcp::{McpDeps, McpOps};
pub use models::{ModelDeps, ModelOps};
pub use proxy::{ProxyDeps, ProxyOps};
pub use servers::{ServerDeps, ServerOps};
pub use settings::{SettingsDeps, SettingsOps};
pub use setup::{SetupDeps, SetupOps};

// Re-export commonly used types from gglib-core for convenience
pub use gglib_core::ModelFilterOptions;
pub use gglib_core::download::QueueSnapshot;
