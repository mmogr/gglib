#![doc = include_str!("README.md")]

//! Command handlers that delegate to AppCore.
//!
//! This module contains the command execution logic for CLI commands.
//!
//! Handlers follow the canonical pattern:
//! - Signature: `pub async fn execute(ctx: &CliContext, ...) -> Result<()>`
//! - Thin wrappers that:
//!   1. Parse/validate CLI-specific input
//!   2. Call AppCore methods
//!   3. Format output for the terminal
//!
//! Handlers should NOT:
//! - Access repositories directly
//! - Contain business logic
//! - Manage database connections

pub mod add;
pub mod chat;
pub mod check_deps;
pub mod config;
pub mod download;
pub mod list;
pub mod paths;
pub mod remove;
pub mod serve;
pub mod update;
