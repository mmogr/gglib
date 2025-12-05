//! Tauri command handlers.
//!
//! This module contains all Tauri commands organized by domain.
//! Commands are thin wrappers that delegate to `GuiBackend` services.
//!
//! # Rules
//!
//! - Commands import only `crate::app::*` and external crates
//! - No cross-command imports
//! - Domain types stay local to their module

mod downloads;
mod huggingface;
mod llama;
mod mcp;
mod models;
mod proxy;
mod servers;
mod settings;
mod tags;
mod util;

// Flat re-exports for invoke_handler! ergonomics
pub use downloads::*;
pub use huggingface::*;
pub use llama::*;
pub use mcp::*;
pub use models::*;
pub use proxy::*;
pub use servers::*;
pub use settings::*;
pub use tags::*;
pub use util::*;
