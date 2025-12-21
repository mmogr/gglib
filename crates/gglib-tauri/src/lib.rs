#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use serde_json as _;
#[cfg(test)]
use tokio_test as _;

// Dependencies used by bootstrap and gui_backend modules
use anyhow as _;
use gglib_db as _;
use gglib_download as _;
use gglib_gui as _;
use gglib_hf as _;
use gglib_runtime as _;
use tokio as _;
use tracing as _;

pub mod bootstrap;
pub mod error;
pub mod event_emitter;
pub mod events;
pub mod gui_backend;
pub mod server_events;

// Re-export primary types
pub use bootstrap::{TauriConfig, TauriContext, bootstrap};
pub use error::TauriError;
pub use event_emitter::TauriEventEmitter;
pub use server_events::TauriServerEvents;

// Re-export GuiError for app crate to use in error mapping
pub use gglib_gui::GuiError;
pub use gui_backend::{GuiBackend, GuiDeps};
