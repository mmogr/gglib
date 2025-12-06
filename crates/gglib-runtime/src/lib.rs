//! Process runtime and OS-level concerns for gglib.
//!
//! This crate provides the `ProcessRunner` implementation for spawning and
//! managing llama-server processes. It contains only OS/process concerns
//! and has no database or domain decision logic.
//!
//! # Design Rules
//!
//! - Implements `gglib_core::ports::ProcessRunner` only
//! - No database access or domain logic
//! - No model resolution — accepts `ServerConfig` and executes it
//! - All types needed by runtime are defined in `gglib-core`
//!
//! # Structure
//!
//! - `runner` - `LlamaServerRunner` implementing `ProcessRunner`
//! - `process_core` - Low-level process spawning and tracking
//! - `health` - HTTP health check utilities

#![deny(unsafe_code)]

mod command;
mod health;
mod process_core;
mod runner;

// Re-export the main ProcessRunner implementation
pub use runner::LlamaServerRunner;

// Re-export health utilities for direct use if needed
pub use health::{check_http_health, wait_for_http_health};
