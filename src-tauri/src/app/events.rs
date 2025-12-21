//! Centralized event emission for Tauri.
//!
//! This module re-exports event helpers from gglib-tauri for use in the app crate.

// Re-export from gglib-tauri to maintain existing import paths
pub use gglib_tauri::events::{emit_or_log, names};
