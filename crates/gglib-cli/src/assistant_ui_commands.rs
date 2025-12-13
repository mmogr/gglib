//! assistant-ui management subcommands.
//!
//! This module defines commands for managing the assistant-ui frontend.

use clap::Subcommand;

/// assistant-ui management commands.
#[derive(Subcommand)]
pub enum AssistantUiCommand {
    /// Install assistant-ui npm dependencies
    Install,
    /// Update assistant-ui dependencies
    Update,
    /// Show assistant-ui installation status
    Status,
}
