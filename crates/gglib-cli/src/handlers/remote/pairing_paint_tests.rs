//! Tests for the pairing screen's painting: nothing painted scrolls or
//! erases the display, a tick rewrites only what changed, and leaving gives
//! the terminal back.
//!
//! Beside `pairing_layout_tests.rs` rather than in it, for the 300-line
//! budget.

use super::super::pairing_tui::qr;
use super::pairing_layout_tests::{frame, offer, painted, pairing};
use super::*;

/// A first paint clears every row a line at a time, puts each line on its
/// own row, and neither scrolls nor erases the display.
#[test]
fn a_first_paint_never_scrolls_and_never_erases_the_display() {
    let frame = frame(80, 4);
    let out = painted(&frame, None);
    for banned in ["\n", "\r", "\x1b[2J", "\x1b[J", "\x1b[0J", "\x1b[3J"] {
        assert!(!out.contains(banned), "{banned:?} in {out:?}");
    }
    assert!(out.starts_with("\x1b[?7l") && out.ends_with("\x1b[?7h"));
    let cleared = "\x1b[1;1H\x1b[2K\x1b[2;1H\x1b[2K\x1b[3;1H\x1b[2K\x1b[4;1H\x1b[2K";
    // The join on the first row with wrap on, and only there; the countdown
    // three rows down, cleared before it is printed.
    let join = &frame.lines[0].text;
    let countdown = &frame.lines[1].text;
    assert_eq!(
        out,
        format!(
            "\x1b[?7l{cleared}\x1b[1;1H\x1b[?7h{join}\x1b[?7l\x1b[4;1H\x1b[2K{countdown}\x1b[?7h"
        )
    );
}

/// A tick that changes only the countdown rewrites only the countdown, so a
/// selection of the join command survives it.
#[test]
fn a_tick_rewrites_only_the_countdown() {
    let pairing = pairing();
    let offer = offer(&pairing, Some("desk"));
    let drawn = qr(&pairing);
    let lay = |size, left| Frame::lay(size, &offer, drawn.as_deref(), left);
    let (was, is) = (lay((200, 60), 120), lay((200, 60), 119));
    let last = is.lines.len() - 1;
    let countdown = &is.lines[last].text;
    assert_eq!(
        painted(&is, Some(&was)),
        format!("\x1b[?7l\x1b[{};1H\x1b[2K{countdown}\x1b[?7h", last + 1)
    );
    assert_eq!(painted(&is, Some(&is)), "\x1b[?7l\x1b[?7h");

    // A window that changed size is painted from nothing.
    for other in [lay((201, 60), 119), lay((200, 20), 119)] {
        assert_eq!(painted(&other, Some(&was)), painted(&other, None));
        let clears = painted(&other, None).matches("\x1b[2K").count();
        assert!(clears >= usize::from(other.size.1));
    }
    // And so is one whose wrapped line is not the one on screen.
    let mut moved = lay((80, 4), 120);
    let narrow = moved.clone();
    moved.lines[0].text.push('x');
    assert_eq!(painted(&narrow, Some(&moved)), painted(&narrow, None));
}

/// What was on screen is trusted only when every line sits where it sat:
/// the same number of lines, wrapped where they were wrapped.
#[test]
fn a_frame_of_another_shape_in_the_same_window_is_painted_from_nothing() {
    let plain = |text: &str| Line {
        text: text.to_owned(),
        wrap: false,
    };
    let frame = |lines: Vec<Line>| Frame {
        size: (40, 6),
        lines,
    };
    let two = frame(vec![plain("one"), plain("two")]);
    let three = frame(vec![plain("one"), plain("two"), plain("three")]);
    assert_eq!(painted(&two, Some(&three)), painted(&two, None));
    assert_eq!(painted(&three, Some(&two)), painted(&three, None));

    let wrapped = frame(vec![
        Line {
            text: "one".to_owned(),
            wrap: true,
        },
        plain("two"),
    ]);
    assert_eq!(painted(&two, Some(&wrapped)), painted(&two, None));
    assert_eq!(painted(&wrapped, Some(&two)), painted(&wrapped, None));
}

#[test]
fn leaving_turns_wrap_back_on_shows_the_cursor_and_leaves_the_alternate_screen() {
    let mut out = Vec::new();
    restore(&mut out).expect("a Vec takes every write");
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "\x1b[?7h\x1b[?25h\x1b[?1049l"
    );
}
