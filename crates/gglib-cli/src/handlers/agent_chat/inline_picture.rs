//! A picture drawn in the terminal itself, by the two protocols terminals
//! have for it.
//!
//! [`Inline::of_terminal`] says which one this terminal reads, from what
//! the terminal itself sets in its environment: there is no asking one that
//! would not also print garbage in a terminal that does not answer. A
//! terminal not named here shows no picture, and its marker is all it had
//! before.
//!
//! - **iTerm2**, from 3.5: its inline image sequence in several parts
//!   (`OSC 1337 ; MultipartFile`), as its own `imgcat` sends one. A PNG or
//!   a JPEG.
//! - **iTerm2 from 3.0 to before 3.5, and `WezTerm`**: the same image in
//!   its older form, one sequence (`OSC 1337 ; File`), which is all
//!   `WezTerm` reads of iTerm2's. A PNG or a JPEG. An iTerm2 whose version is not
//!   said is not sent either form.
//! - **kitty and Ghostty**: the kitty graphics protocol, which takes a PNG
//!   as it is (`f=100`). A JPEG is not sent; its marker stands.
//!
//! Inside tmux or screen neither is sent: both need wrapping there, and a
//! multiplexer that does not pass it on prints it.

use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use gglib_core::request_pipeline::PNG_MIME;

/// The most base64 characters one escape sequence carries: the kitty
/// protocol's limit, and a multiple of four as it asks.
const PART: usize = 4096;

/// The widest a picture is drawn, in columns.
const WIDEST: u32 = 40;

/// The tallest a picture is drawn, in rows.
const TALLEST: u32 = 20;

/// How many of a picture's pixels a column is taken to cover, so a small
/// picture is not blown up: about a cell's width on an ordinary display.
const COLUMN_PX: u32 = 8;

/// The protocol a terminal draws a picture by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Inline {
    /// iTerm2's inline images, multipart.
    ITerm,
    /// iTerm2's inline images in one sequence: iTerm2 before 3.5, and
    /// `WezTerm`.
    ITermSingle,
    /// The kitty graphics protocol: kitty and Ghostty.
    Kitty,
}

impl Inline {
    /// The protocol the terminal with this environment (`env`) reads, if it
    /// is one known to draw pictures.
    pub(crate) fn of_terminal(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let set = |name: &str| env(name).is_some_and(|value| !value.is_empty());
        let is = |name: &str, value: &str| env(name).as_deref() == Some(value);
        let term = env("TERM").unwrap_or_default();
        if set("TMUX") || set("STY") || term.starts_with("screen") || term.starts_with("tmux") {
            return None;
        }
        let iterm = |version: &str| {
            let version = env(version)?;
            if at_least(&version, (3, 5)) {
                Some(Self::ITerm)
            } else {
                at_least(&version, (3, 0)).then_some(Self::ITermSingle)
            }
        };
        // A terminal is believed by what it sets itself. One started from
        // another (an editor's, from a shell in iTerm2 or kitty) inherits
        // that one's variables, and would be sent a picture it prints.
        match env("TERM_PROGRAM").unwrap_or_default().as_str() {
            "iTerm.app" => iterm("TERM_PROGRAM_VERSION"),
            "ghostty" => Some(Self::Kitty),
            "WezTerm" => Some(Self::ITermSingle),
            // No program named: kitty names none, and none travels to a
            // shell on another machine. `TERM` does, and so does what
            // iTerm2 sets for that case, `LC_TERMINAL`.
            "" if term == "xterm-kitty" || term == "xterm-ghostty" => Some(Self::Kitty),
            "" if is("LC_TERMINAL", "iTerm2") && (set("SSH_CONNECTION") || set("SSH_TTY")) => {
                iterm("LC_TERMINAL_VERSION")
            }
            _ => None,
        }
    }

    /// What to write to draw `picture`, `columns` wide, the cursor left on
    /// the line after it. None for a picture this protocol does not take,
    /// and for one with no bytes.
    pub(crate) fn drawn(self, picture: &Picture<'_>, columns: u32) -> Option<String> {
        if picture.bytes.is_empty() {
            return None;
        }
        let encoded = STANDARD.encode(picture.bytes);
        let mut parts = encoded.as_bytes().chunks(PART).peekable();
        let mut out = String::with_capacity(encoded.len() + encoded.len() / 64 + 256);
        match self {
            Self::ITerm => {
                let arguments = iterm_arguments(picture, columns);
                let _ = write!(out, "\x1b]1337;MultipartFile={arguments}\x07");
                for part in parts {
                    let _ = write!(out, "\x1b]1337;FilePart={}\x07", text(part));
                }
                out.push_str("\x1b]1337;FileEnd\x07");
            }
            Self::ITermSingle => {
                let arguments = iterm_arguments(picture, columns);
                let _ = write!(out, "\x1b]1337;File={arguments}:{encoded}\x07");
            }
            Self::Kitty => {
                if picture.mime != PNG_MIME {
                    return None;
                }
                // `q=2`: the terminal answers nothing, where an answer would
                // arrive as typed input.
                let mut keys = format!("a=T,f=100,q=2,c={columns},");
                while let Some(part) = parts.next() {
                    let more = u8::from(parts.peek().is_some());
                    let _ = write!(out, "\x1b_G{keys}m={more};{}\x1b\\", text(part));
                    keys.clear();
                }
            }
        }
        out.push('\n');
        Some(out)
    }
}

/// A stored picture, to draw.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Picture<'a> {
    /// What it is called, where a terminal shows a name.
    pub name: &'a str,
    /// `image/png` or `image/jpeg`.
    pub mime: &'a str,
    /// The file's bytes.
    pub bytes: &'a [u8],
}

/// What iTerm2's image sequence says of `picture`, in either form: shown
/// inline, its size and name, `columns` wide and as tall as that makes it.
fn iterm_arguments(picture: &Picture<'_>, columns: u32) -> String {
    let (size, name) = (picture.bytes.len(), STANDARD.encode(picture.name));
    format!("inline=1;size={size};name={name};width={columns};preserveAspectRatio=1")
}

/// Base64 is ASCII, so any run of its bytes is text.
fn text(part: &[u8]) -> &str {
    std::str::from_utf8(part).unwrap_or_default()
}

/// Whether `version` ("3.7.7", "3.5.0beta10") is `least` (major, minor) or
/// later. One that does not start with two numbers is not.
fn at_least(version: &str, least: (u32, u32)) -> bool {
    let mut numbers = version.split('.').map(|part| {
        let digits = part.bytes().take_while(u8::is_ascii_digit).count();
        part[..digits].parse::<u32>().ok()
    });
    match (numbers.next().flatten(), numbers.next().flatten()) {
        (Some(major), Some(minor)) => (major, minor) >= least,
        _ => false,
    }
}

/// How many columns to draw a `width` by `height` picture over, in a window
/// `window` columns wide when that is known, which a width of none is not
/// (a terminal that was never told its size says 0): at most [`WIDEST`], no wider
/// than the window or than the picture's own pixels fill, and no taller than
/// [`TALLEST`] rows, a cell being about twice as tall as it is wide.
pub(crate) fn columns(width: u32, height: u32, window: Option<u16>) -> u32 {
    let by_window = window
        .filter(|columns| *columns > 0)
        .map_or(WIDEST, |columns| u32::from(columns).saturating_sub(2));
    let by_pixels = width.div_ceil(COLUMN_PX);
    let by_height = (TALLEST * 2).saturating_mul(width) / height.max(1);
    WIDEST.min(by_window).min(by_pixels).min(by_height).max(1)
}

#[cfg(test)]
#[path = "inline_picture_tests.rs"]
mod inline_picture_tests;
