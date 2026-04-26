//! GUI backend bridge module - re-exports from gglib-gui.
//!
//! This module is a thin bridge that re-exports types from the shared
//! `gglib-gui` crate. Tauri command handlers import from here.

// Re-export all types from gglib-gui
pub use gglib_app_services::types::*;

// Re-export GuiError for error conversion
pub use gglib_app_services::GuiError;

// Re-export QueueSnapshot which is used by download commands
pub use gglib_core::download::QueueSnapshot;

// Re-export ModelFilterOptions which is used by model commands
pub use gglib_core::ModelFilterOptions;
