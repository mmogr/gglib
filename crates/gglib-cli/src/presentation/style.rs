//! ANSI terminal colour and style constants.
//!
//! Centralised source of truth for all escape sequences used by CLI output.
//! Import with `use crate::presentation::style::*;` in handler modules.

use crossterm::style::{Attribute, Color};
use termimad::MadSkin;

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
/// Dim — reduced intensity (thinking blocks).
pub const DIM: &str = "\x1b[2m";
/// Resets all attributes.
pub const RESET: &str = "\x1b[0m";

/// Build a [`MadSkin`] tuned for dark terminal backgrounds.
///
/// Headings are bold cyan, inline code is yellow, and code blocks use green
/// foreground with a 2-column left margin.
pub fn get_markdown_skin() -> MadSkin {
    let mut skin = MadSkin::default_dark();
    skin.set_headers_fg(Color::Cyan);
    for h in &mut skin.headers {
        h.add_attr(Attribute::Bold);
    }
    skin.inline_code.set_fg(Color::Yellow);
    skin.code_block.set_fg(Color::Green);
    skin.code_block.left_margin = 2;
    skin
}
