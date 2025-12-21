#![doc = include_str!("README.md")]

//! Shared CLI presentation utilities.
//!
//! This module provides reusable display and formatting functions
//! for consistent CLI output across commands.
//!
//! # Guidelines
//!
//! - Keep this module format-only: no domain transforms
//! - Domain transforms belong in core services or CLI-local view-model helpers

pub mod model_display;
pub mod tables;

// Re-export commonly used items
pub use model_display::{DisplayStyle, ModelSummaryOpts, display_model_summary};
pub use tables::{format_optional, print_separator, truncate_string};
