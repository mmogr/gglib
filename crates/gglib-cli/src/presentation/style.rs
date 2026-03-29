//! ANSI terminal colour and style constants.
//!
//! Centralised source of truth for all escape sequences used by CLI output.
//! Import with `use crate::presentation::style::*;` in handler modules.

/// Green — success states, installed dependencies, GPU detected.
pub const SUCCESS: &str = "\x1b[32m";
/// Red — error states, missing required dependencies.
pub const DANGER: &str = "\x1b[31m";
/// Yellow — warnings, optional missing items.
pub const WARNING: &str = "\x1b[33m";
/// Blue — informational labels, commands, headings.
pub const INFO: &str = "\x1b[34m";
/// Bold — emphasis, table headers.
pub const BOLD: &str = "\x1b[1m";
/// Resets all attributes.
pub const RESET: &str = "\x1b[0m";
