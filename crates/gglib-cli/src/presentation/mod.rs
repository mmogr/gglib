#![doc = include_str!("README.md")]

pub(crate) mod capability_flags;
pub(crate) mod explain_display;
#[allow(
    clippy::option_if_let_else,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod inspect_display;
pub(crate) mod model_display;
pub(crate) mod sampling_values;
pub(crate) mod style;
pub(crate) mod tables;

// Re-export commonly used items
pub(crate) use model_display::{ModelSummaryOpts, display_model_summary};
pub(crate) use tables::{
    first_chars, format_number, format_relative_time, print_separator, short_sha, truncate_string,
    truncate_with,
};
