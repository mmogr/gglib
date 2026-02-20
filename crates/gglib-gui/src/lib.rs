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
mod voice;

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
pub use voice::VoiceOps;

// Re-export commonly used types from gglib-core for convenience
pub use gglib_core::ModelFilterOptions;
pub use gglib_core::download::QueueSnapshot;
