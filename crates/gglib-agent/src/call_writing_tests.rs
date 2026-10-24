//! A reply's `tool_call_writing` reports, with the clock as an argument:
//! the first piece that names a tool at once, then at most one a second.

use std::time::Duration;

use gglib_core::AgentEvent;
use tokio::time::Instant;

use super::CallWritingReports;

const TOOL: &str = "builtin:generate_image";

/// The `tokens` of each report `pieces` sent, a piece being (milliseconds
/// after the first, whether its call is named yet).
fn reported(pieces: &[(u64, bool)]) -> Vec<u32> {
    let start = Instant::now();
    let mut reports = CallWritingReports::default();
    pieces
        .iter()
        .filter_map(|&(ms, named)| {
            let at = start + Duration::from_millis(ms);
            reports.piece(named.then_some(TOOL), at)
        })
        .map(|event| match event {
            AgentEvent::ToolCallWriting(writing) => writing.tokens,
            other => panic!("not a writing report: {other:?}"),
        })
        .collect()
}

#[test]
fn the_first_named_piece_is_reported_and_names_the_tool() {
    let mut reports = CallWritingReports::default();
    let Some(AgentEvent::ToolCallWriting(writing)) = reports.piece(Some(TOOL), Instant::now())
    else {
        panic!("the first piece is reported");
    };
    assert_eq!(writing.tool_name, TOOL);
    assert_eq!(writing.display_name, "Generate Image");
    assert_eq!(writing.tokens, 1);
}

/// A thousand pieces in the same instant are one report, not a thousand.
#[test]
fn pieces_within_a_second_of_a_report_are_not_reported() {
    let pieces: Vec<(u64, bool)> = (0..1000).map(|_| (0, true)).collect();
    assert_eq!(reported(&pieces), [1]);
}

/// A piece a second or more after the last report is reported, with every
/// piece counted, the unreported ones and those before the name included.
#[test]
fn a_piece_a_second_after_the_last_report_is_reported_with_all_pieces_counted() {
    let pieces = [
        (0, false),
        (100, true),
        (600, true),
        (1099, true),
        (1100, true),
        (1500, true),
        (2100, true),
    ];
    assert_eq!(reported(&pieces), [2, 5, 7]);
}

#[test]
fn a_call_no_piece_has_named_is_not_reported() {
    assert!(reported(&[(0, false), (5000, false)]).is_empty());
}
