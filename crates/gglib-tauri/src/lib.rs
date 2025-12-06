//! Tauri GUI backend adapter for gglib.
//!
//! This crate provides the Tauri command handlers and event emitters
//! for the gglib desktop application.
//!
//! # Architecture
//!
//! - `bootstrap` - Composition root (wires repos + runtime + services)
//! - `error` - Tauri-specific error types with JSON serialization
//! - `gui_backend` - Shared GUI backend service (re-exported from gglib)
//! - `commands/` - Tauri command handlers (to be added)
//! - `events` - Event emitters for frontend notifications (to be added)

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use serde_json as _;
#[cfg(test)]
use tokio_test as _;

// Dependencies used by bootstrap module
use anyhow as _;
use gglib as _;
use gglib_db as _;
use gglib_download as _;
use gglib_hf as _;
use gglib_runtime as _;
use tracing as _;

pub mod bootstrap;
pub mod error;
pub mod gui_backend;

// Re-export primary types
pub use bootstrap::{TauriConfig, TauriContext, bootstrap};
pub use error::TauriError;
pub use gui_backend::GuiBackend;
