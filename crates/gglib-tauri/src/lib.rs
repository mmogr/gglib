//! Tauri GUI backend adapter for gglib.
//!
//! This crate provides the Tauri command handlers and event emitters
//! for the gglib desktop application.
//!
//! # Architecture
//!
//! - `error` - Tauri-specific error types with JSON serialization
//! - `commands/` - Tauri command handlers (to be added)
//! - `events` - Event emitters for frontend notifications (to be added)

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use tokio_test as _;
#[cfg(test)]
use serde_json as _;

// gglib-db will be used by command handlers as they are migrated
use gglib_db as _;

pub mod error;

// Re-export primary types
pub use error::TauriError;
