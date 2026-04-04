//! ANSI terminal colour and style constants.
//!
//! Centralised source of truth for all escape sequences used by CLI output.
//! Import with `use crate::presentation::style::*;` in handler modules.

use crossterm::style::{Attribute, Color};
use termimad::{ListItemsIndentationMode, MadSkin, StyledChar};

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

    // Explicit bold/italic so rendering is consistent across terminals.
    skin.bold.set_fg(Color::White);
    skin.bold.add_attr(Attribute::Bold);
    skin.italic.add_attr(Attribute::Italic);

    // Consistent bullet character and prettier multi-line wrapping.
    skin.bullet = StyledChar::from_fg_char(Color::Cyan, '•');
    skin.list_items_indentation_mode = ListItemsIndentationMode::Block;
    skin
}

// ─────────────────────────────────────────────────────────────────────────────
// Banners
// ─────────────────────────────────────────────────────────────────────────────

/// Print a styled banner to stderr and enter DIM mode for body text.
///
/// ```text
///   ╭─ ℹ️  Info ─────────────────────────╮
///   (body text appears dimmed)
///   ╰───────────────────────────────────────╯
/// ```
///
/// The border is rendered in [`INFO`] blue, then DIM is activated so that
/// subsequent output renders in reduced intensity.  Call
/// [`print_banner_close`] when the body is complete.
///
/// All output goes to **stderr** so that stdout remains clean for piped
/// command output.
pub fn print_info_banner(label: &str, emoji: &str) {
    // "  ╭─ {emoji} {label} " = fixed prefix; fill the rest with ─ up to
    // column 42 (banner width), then close with ╮.
    let prefix = format!("  \u{256d}\u{2500} {emoji} {label} ");
    let fill_len = 42usize.saturating_sub(prefix.chars().count());
    let fill = "\u{2500}".repeat(fill_len);
    eprintln!("\n{INFO}{prefix}{fill}\u{256e}{RESET}");
    eprint!("{DIM}");
}

/// Print the bottom border of a banner and reset all ANSI attributes.
///
/// ```text
///   ╰───────────────────────────────────────╯
/// ```
pub fn print_banner_close() {
    // 39 = banner width (42) minus the 3-char "  ╰" prefix.
    let fill = "\u{2500}".repeat(39);
    eprintln!("{RESET}\n{INFO}  \u{2570}{fill}\u{256f}{RESET}");
}

/// Print the thinking-block banner and enter DIM mode on stderr.
///
/// Equivalent to [`print_info_banner`] with label "Thinking" and 💭 emoji.
pub fn print_thinking_banner() {
    print_info_banner("Thinking", "\u{1f4ad}");
}
