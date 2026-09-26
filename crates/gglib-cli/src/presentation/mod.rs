#![doc = include_str!("README.md")]

pub(crate) mod explain_display;
pub(crate) mod inspect_display;
pub(crate) mod model_display;
pub(crate) mod style;
pub(crate) mod tables;

// Re-export commonly used items
pub(crate) use model_display::{ModelSummaryOpts, display_model_summary};
pub(crate) use tables::{format_relative_time, print_separator, truncate_string};
