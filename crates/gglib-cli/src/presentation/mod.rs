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

pub(crate) mod explain_display;
pub(crate) mod inspect_display;
pub(crate) mod model_display;
pub(crate) mod style;
pub(crate) mod tables;

// Re-export commonly used items
pub(crate) use model_display::{ModelSummaryOpts, display_model_summary};
pub(crate) use tables::{format_relative_time, print_separator, truncate_string};
