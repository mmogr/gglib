//! Which figure line a reply prints after itself, and whether.

use gglib_core::domain::agent::{AgentEvent, TurnUsage};

use super::ReplyFigures;
use crate::presentation::style::{DIM, RESET};

/// The turn's `turn_usage` frame as `contracts/runs/turn_made.json` pins it.
fn recorded_usage() -> TurnUsage {
    let made: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../contracts/runs/turn_made.json")).unwrap();
    made["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|frame| match serde_json::from_value(frame.clone()) {
            Ok(AgentEvent::TurnUsage(usage)) => Some(usage),
            _ => None,
        })
        .expect("the contract's turn_usage frame")
}

/// A tool round's call first, then the answering call: the reply prints the
/// answering call's line, in the GUI thread's words.
#[test]
fn a_reply_prints_its_last_calls_figure_line() {
    let tool_round = TurnUsage {
        completion_tokens: Some(30),
        duration_ms: 2_000,
        writing_ms: Some(1_000),
        ..TurnUsage::default()
    };
    let mut figures = ReplyFigures::default();
    figures.saw(&tool_round);
    figures.saw(&recorded_usage());
    assert_eq!(
        figures.line(false).as_deref(),
        Some(format!("  {DIM}41 s \u{b7} 13 t/s \u{b7} 2,100 tokens reused{RESET}").as_str())
    );
}

/// `--quiet` prints only the answer, and a reply whose calls sent no usage
/// has no line to print.
#[test]
fn quiet_or_without_usage_prints_no_line() {
    let mut figures = ReplyFigures::default();
    assert_eq!(figures.line(false), None);
    figures.saw(&recorded_usage());
    assert_eq!(figures.line(true), None);
}
