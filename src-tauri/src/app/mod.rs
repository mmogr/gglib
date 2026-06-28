#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
//! Application state and shared utilities for the Tauri app.

pub mod events;
pub mod state;

pub use state::AppState;
