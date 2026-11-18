//! Which figure line a reply prints after itself, and whether; and where
//! the text a model writes beside a tool call is shown.

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

/// A turn in which the model writes "A cat!" beside its tool call, and
/// "I drew a cat." after the tool.
fn a_tool_round_with_text() -> Vec<AgentEvent> {
    use gglib_core::domain::agent::{ToolCall, ToolResult};

    let text = |content: &str| AgentEvent::TextDelta {
        content: content.to_owned(),
    };
    vec![
        text("A cat"),
        text("!"),
        AgentEvent::ToolCallStart {
            tool_call: ToolCall {
                id: "c1".to_owned(),
                name: "builtin:generate_image".to_owned(),
                arguments: serde_json::json!({}),
            },
            display_name: "Generate Image".to_owned(),
            args_summary: None,
            server: None,
        },
        AgentEvent::ToolCallComplete {
            tool_name: "builtin:generate_image".to_owned(),
            result: ToolResult::text("c1", "Drew 1 image", true),
            wait_ms: 0,
            execute_duration_ms: 1,
            display_name: "Generate Image".to_owned(),
            duration_display: "1ms".to_owned(),
        },
        AgentEvent::IterationComplete {
            iteration: 0,
            tool_calls: 1,
        },
        text("I drew a cat."),
        AgentEvent::FinalAnswer {
            content: "I drew a cat.".to_owned(),
        },
    ]
}

/// What `events` write to stdout in the mode `rich` names: each [`Said`]
/// in order, a block as `[text]`.
async fn said(events: Vec<AgentEvent>, rich: bool) -> String {
    use super::{RenderContext, Said, drain};

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    for event in events {
        tx.send(event).unwrap();
    }
    drop(tx);
    let mut written = String::new();
    // Quiet in Raw mode, as a pipe's `--quiet` is; Rich mode is never quiet.
    let mut ctx = RenderContext::new(rich, false, !rich);
    let answered = drain(&mut rx, false, &mut ctx, &mut |said| match said {
        Said::Block(text) => {
            written.push('[');
            written.push_str(text);
            written.push(']');
        }
        Said::Raw(text) => written.push_str(text),
    })
    .await;
    assert!(answered);
    written
}

/// On a terminal, text written beside a tool call is its own block, shown
/// when the tool starts, and the reply after the tool is another: the two
/// are not joined and held to the end.
#[tokio::test]
async fn text_beside_a_tool_call_is_a_block_of_its_own() {
    assert_eq!(
        said(a_tool_round_with_text(), true).await,
        "[A cat!][I drew a cat.]"
    );
}

/// To a pipe, the same turn's two texts are a blank line apart.
#[tokio::test]
async fn text_beside_a_tool_call_is_a_blank_line_from_the_reply_in_a_pipe() {
    assert_eq!(
        said(a_tool_round_with_text(), false).await,
        "A cat!\n\nI drew a cat.\n"
    );
}

/// The spinner that counts buffered text stops when that text is ended, so
/// a tool's progress line is not redrawn over; and ending twice shows once.
#[test]
fn ending_text_stops_its_spinner_and_shows_it_once() {
    use super::{RenderContext, Said, emit_content, end_text};

    let mut blocks = Vec::new();
    let mut out = |said: Said<'_>| match said {
        Said::Block(text) | Said::Raw(text) => blocks.push(text.to_owned()),
    };
    let mut ctx = RenderContext::new(true, false, false);
    emit_content(&mut ctx, "A cat!", &mut out);
    assert!(ctx.spinner.is_some());

    end_text(&mut ctx, &mut out);
    end_text(&mut ctx, &mut out);

    assert!(ctx.spinner.is_none());
    assert_eq!(blocks, ["A cat!"]);
}

/// The text is shown when the tool starts, not when its round ends: with
/// the turn's events read as far as the tool's start and the stream still
/// open, the text beside the call is already out.
#[tokio::test]
async fn text_beside_a_tool_call_is_out_once_the_tool_starts() {
    use std::sync::Mutex;

    use super::{RenderContext, Said, drain};

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    for event in a_tool_round_with_text().into_iter().take(3) {
        tx.send(event).unwrap();
    }
    let written = Mutex::new(String::new());
    let mut ctx = RenderContext::new(true, false, false);
    let mut out = |said: Said<'_>| {
        if let Said::Block(text) = said {
            written.lock().unwrap().push_str(text);
        }
    };
    let mut draining = std::pin::pin!(drain(&mut rx, false, &mut ctx, &mut out));

    assert!(futures_util::poll!(draining.as_mut()).is_pending());

    assert_eq!(*written.lock().unwrap(), "A cat!");
    drop(tx);
}
