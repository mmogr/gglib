#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
//! Transport contract constants.
//!
//! This module contains string constants for API routes and command names
//! shared across adapters (Axum, Tauri). Keep these string-only with no
//! framework-specific types to avoid dependency creep.

pub mod http;
