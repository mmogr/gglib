//! Tests for the pairing screen's layout and painting: no frame is taller
//! than its window, and nothing painted scrolls or erases the display.

use std::io;

use super::super::pairing_tui::qr;
use super::*;

/// A pairing as long as a live one: a ticket carrying a relay and an address
/// comes to about 142 characters, and the code adds seven.
pub(super) fn pairing() -> String {
    format!("pipe{}-483920", "a2b3c4d5e6f7".repeat(12).split_at(138).0)
}

/// The rows and columns `qr(pairing())` draws, indent included. Pinned, so
/// that a change to the renderer shows here before it shows in a window.
const QR: (usize, usize) = (25, 51);

pub(super) fn offer(pairing: &str, device: Option<&str>) -> RemoteEnableResponse {
    let mut answer = serde_json::json!({
        "ticket": "pipeaaaa",
        "code": "483920",
        "pairing": pairing,
        "expires_in_s": 120,
        "mcp_allowed": false,
    });
    if let Some(device) = device {
        answer["device"] = device.into();
    }
    serde_json::from_value(answer).expect("an answer the daemon could send")
}

/// The frame for a live-sized pairing, for a device called `desk`.
pub(super) fn frame(cols: usize, rows: usize) -> Frame {
    let size = (u16::try_from(cols).unwrap(), u16::try_from(rows).unwrap());
    let pairing = pairing();
    Frame::lay(
        size,
        &offer(&pairing, Some("desk")),
        qr(&pairing).as_deref(),
        120,
    )
}

pub(super) fn texts(frame: &Frame) -> Vec<&str> {
    frame.lines.iter().map(|l| l.text.as_str()).collect()
}

pub(super) fn painted(frame: &Frame, shown: Option<&Frame>) -> String {
    let mut out = Vec::new();
    paint(&mut out, frame, shown).expect("a Vec takes every write");
    String::from_utf8(out).expect("the screen is text")
}

/// The rows a paint moved the cursor to, in order.
pub(super) fn moves(painted: &str) -> Vec<usize> {
    painted
        .split("\x1b[")
        .filter_map(|rest| {
            rest.split_once('H')?
                .0
                .split_once(';')?
                .0
                .parse::<usize>()
                .ok()
        })
        .map(|row| row - 1)
        .collect()
}

#[test]
fn the_qr_for_a_live_sized_pairing_is_the_size_pinned_here() {
    let drawn = qr(&pairing()).expect("fits a QR");
    let cols = drawn.lines().map(|l| l.chars().count()).max().unwrap() + 2;
    assert_eq!((drawn.lines().count(), cols), QR);
}

/// The sweep: whatever the window, the frame fits it, says something, carries
/// no control character, and wraps nothing but an ASCII join command.
#[test]
fn no_frame_is_taller_than_its_window() {
    let pairing = pairing();
    let drawn = qr(&pairing);
    let mut joins = 0;
    for device in [None, Some("desk"), Some("\u{1f5a5}\u{fe0f} desk\x1b[?7h")] {
        let offer = offer(&pairing, device);
        let sizes = (1..=200_u16).flat_map(|cols| (1..=60_u16).map(move |rows| (cols, rows)));
        for ((cols, rows), left) in sizes.flat_map(|size| [0, 7, 120].map(|left| (size, left))) {
            let frame = Frame::lay((cols, rows), &offer, drawn.as_deref(), left);
            let at = format!("{cols}x{rows}");
            assert!(!frame.lines.is_empty(), "{at} is blank");
            // Counted here from the text, not by `Line::rows`.
            let height: usize = frame
                .lines
                .iter()
                .map(|l| {
                    if l.wrap {
                        l.text.len() / usize::from(cols) + 1
                    } else {
                        1
                    }
                })
                .sum();
            assert!(height <= usize::from(rows), "{at} is {height} rows");
            for line in &frame.lines {
                assert!(!line.text.contains(char::is_control), "{at}: {line:?}");
                assert!(!line.wrap || line.text.is_ascii(), "{at}: {line:?}");
                if line.text.contains("gglib remote join") {
                    joins += 1;
                    assert!(line.text.ends_with(&pairing), "{at} cut the pairing");
                }
            }
            assert!(frame.lines.iter().filter(|l| l.wrap).count() <= 1, "{at}");
            let rows_moved_to = moves(&painted(&frame, None));
            assert!(!rows_moved_to.is_empty(), "{at} moved nowhere");
            assert!(
                rows_moved_to.iter().all(|&row| row < usize::from(rows)),
                "{at}"
            );
        }
    }
    assert!(joins > 1000, "the sweep has to meet the join command");
}

/// What is dropped as the window shrinks, in order: the text around the QR,
/// then the QR for the join command, then everything.
#[test]
fn a_smaller_window_gets_the_next_frame_down() {
    let (q, w) = QR;
    let full = frame(200, q + 12);
    assert_eq!(full.lines.len(), q + 12);
    assert!(full.lines[0].text.contains("pair a device"));
    assert!(texts(&full).contains(&"  device  desk"));
    assert!(texts(&full).contains(&"  ticket  pipeaaaa"));
    assert!(
        !full.lines.iter().any(|l| l.wrap),
        "200 columns fit the join"
    );

    let with_join = frame(200, q + 11);
    assert_eq!(with_join.lines.len(), q + 2);
    assert!(with_join.lines[q].text.ends_with(&pairing()));
    assert!(with_join.lines[q + 1].text.contains("120s left"));

    let with_countdown = frame(200, q + 1);
    assert_eq!(with_countdown.lines.len(), q + 1);
    assert!(with_countdown.lines[q].text.contains("120s left"));

    assert_eq!(frame(200, q).lines.len(), q, "the QR alone");
    assert_eq!(frame(w, q).lines.len(), q, "the QR at its exact width");

    // One row or one column short of the QR: the text, saying what it needs.
    for hidden in [frame(200, q - 1), frame(w - 1, 60)] {
        let lines = texts(&hidden);
        assert_eq!(
            lines[0],
            format!("  QR hidden: it needs a window {w} wide and {q} tall.")
        );
        assert!(lines[1].ends_with(&pairing()));
        assert_eq!(lines[2], "  code    483920");
        assert!(lines[3].contains("120s left"));
        assert_eq!(lines.len(), 4);
    }

    // At 80 columns the join command is 167 characters: three rows.
    assert_eq!(frame(80, 6).lines.len(), 4);
    let bare = frame(80, 4);
    assert_eq!(bare.lines.len(), 2);
    assert!(bare.lines[0].wrap && bare.lines[0].text.starts_with("gglib remote join"));
    assert_eq!(texts(&frame(80, 3)), [TOO_SMALL]);
    assert_eq!(texts(&frame(20, 1)), [TOO_SMALL]);
}

#[test]
fn a_wrapped_line_that_ends_at_the_edge_counts_a_row_over() {
    let line = |len| Line {
        text: "x".repeat(len),
        wrap: true,
    };
    assert_eq!(line(159).rows(80), 2);
    assert_eq!(line(160).rows(80), 3);
    assert_eq!(Line::plain("x".repeat(500)).rows(80), 1);
}

/// The join command is indented only when it ends short of the last column.
#[test]
fn the_join_command_is_whole_or_absent() {
    let fits = join_line("pipeaaaa-483920", 38).expect("a pairing");
    assert_eq!((fits.text.len(), fits.wrap), (37, false));
    let wraps = join_line("pipeaaaa-483920", 37).expect("a pairing");
    assert_eq!(
        (wraps.text.as_str(), wraps.wrap),
        ("gglib remote join pipeaaaa-483920", true)
    );
    for not_one in ["", "pipe aaaa", "pipe\u{e9}", "pipe\x1b[2J"] {
        assert_eq!(join_line(not_one, 80), None);
        let frame = Frame::lay(
            (200, 60),
            &offer(not_one, None),
            qr("PIPEAAAA").as_deref(),
            9,
        );
        assert!(
            !texts(&frame).iter().any(|l| l.contains("join")),
            "{not_one:?}"
        );
        assert!(frame.lines.last().unwrap().text.contains("9s left"));
        assert_eq!(
            texts(&Frame::lay((200, 60), &offer(not_one, None), None, 9)),
            [TOO_SMALL]
        );
    }
}

#[test]
fn a_device_name_cannot_carry_an_escape_sequence() {
    let pairing = pairing();
    let offer = offer(&pairing, Some("desk\x1b[?7h"));
    let frame = Frame::lay((200, 60), &offer, qr(&pairing).as_deref(), 120);
    assert!(texts(&frame).contains(&"  device  desk?[?7h"));
}

/// The ticket and the code are the daemon's too. A ticket is shown whole or
/// not at all: cut at the edge it would read as a whole one that is wrong.
#[test]
fn the_ticket_and_the_code_are_cleaned_and_a_ticket_too_long_is_left_out() {
    let pairing = pairing();
    let mut offer = offer(&pairing, None);
    offer.ticket = "pipe\x1b[2Jaa\n".to_owned();
    offer.code = Some("48\n3920".to_owned());
    let frame = Frame::lay((200, 60), &offer, qr(&pairing).as_deref(), 120);
    assert!(texts(&frame).contains(&"  ticket  pipe?[2Jaa?"));
    assert!(texts(&frame).contains(&"  code    48?3920"));

    // Ten columns of label and 190 of ticket end exactly at the edge of 200.
    offer.ticket = "a".repeat(190);
    let cut = Frame::lay((200, 60), &offer, qr(&pairing).as_deref(), 120);
    assert!(cut.lines[0].text.contains("pair a device"));
    assert!(!texts(&cut).iter().any(|l| l.contains("ticket")));
    let whole = Frame::lay((201, 60), &offer, qr(&pairing).as_deref(), 120);
    assert!(texts(&whole).contains(&format!("  ticket  {}", offer.ticket).as_str()));
}

/// A pairing too long for any QR has none to hide, and says nothing of one.
#[test]
fn with_no_qr_at_all_the_text_stands_alone() {
    let pairing = pairing();
    let frame = Frame::lay((200, 60), &offer(&pairing, None), None, 120);
    let lines = texts(&frame);
    assert_eq!(lines.len(), 3);
    assert!(lines[0].ends_with(&pairing));
    assert_eq!(lines[1], "  code    483920");
    assert!(lines[2].contains("120s left"));
}

#[test]
fn a_terminal_that_gives_no_size_is_taken_for_eighty_by_twenty_four() {
    assert_eq!(usable(Ok((132, 43))), (132, 43));
    assert_eq!(usable(Ok((0, 43))), (80, 24));
    assert_eq!(usable(Ok((132, 0))), (80, 24));
    assert_eq!(usable(Err(io::Error::other("not a terminal"))), (80, 24));
}
