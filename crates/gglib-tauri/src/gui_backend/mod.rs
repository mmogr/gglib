//! GUI backend bridge module - re-exports from gglib-gui.
//!
//! This module is a thin bridge that re-exports types and the GuiBackend
//! from the shared `gglib-gui` crate. Tauri command handlers import from
//! here, maintaining existing import paths while delegating to shared code.

// Re-export all types from gglib-gui
pub use gglib_gui::types::*;

// Re-export the core types
pub use gglib_gui::{GuiBackend, GuiDeps, GuiError};

// Re-export QueueSnapshot which is used by download commands
pub use gglib_core::download::QueueSnapshot;

// Re-export ModelFilterOptions which is used by model commands
pub use gglib_core::ModelFilterOptions;
