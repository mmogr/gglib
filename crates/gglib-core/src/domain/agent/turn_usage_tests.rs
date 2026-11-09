//! A turn's writing rate, on the wire and in its figure line.

use super::{ContextReading, TurnUsage};
use crate::domain::agent::AgentEvent;

fn written(completion: u32, writing_ms: u64) -> TurnUsage {
    TurnUsage {
        completion_tokens: Some(completion),
        duration_ms: 41_000,
        writing_ms: Some(writing_ms),
        ..TurnUsage::default()
    }
}

/// The rate rides beside the counts it is made from, and reading the frame
/// back gives the same turn: the rate is never kept apart from them.
#[test]
fn the_rate_is_sent_beside_its_counts_and_read_back_from_them() {
    let usage = written(496, 38_200);
    let json = serde_json::to_value(AgentEvent::TurnUsage(usage.clone())).unwrap();
    assert_eq!(json["tokens_per_second"], serde_json::json!(496.0 / 38.2));
    assert_eq!(json["completion_tokens"], 496);
    assert_eq!(json["writing_ms"], 38_200);
    let back: TurnUsage = serde_json::from_value(json).unwrap();
    assert_eq!(back, usage);
}

/// A turn of one token, or one with no time writing, sends no rate at all.
#[test]
fn a_turn_without_time_writing_sends_no_rate() {
    for usage in [written(1, 0), written(40, 0), TurnUsage::default()] {
        let json = serde_json::to_value(&usage).unwrap();
        assert!(json.get("tokens_per_second").is_none(), "{json}");
    }
}

#[test]
fn the_figure_line_says_the_time_the_rate_and_the_cache() {
    let usage = TurnUsage {
        cached_tokens: Some(2100),
        reading: ContextReading::new(Some(8192), 0),
        ..written(496, 38_200)
    };
    assert_eq!(
        usage.figure_line(),
        "41 s \u{b7} 13 t/s \u{b7} 2,100 tokens reused"
    );
}

/// A figure the turn lacks is left out, never said as zero: no cache count,
/// no reuse; no time writing, no rate.
#[test]
fn the_figure_line_leaves_out_what_the_turn_lacks() {
    let usage = TurnUsage {
        duration_ms: 4_700,
        ..TurnUsage::default()
    };
    assert_eq!(usage.figure_line(), "4.7 s");
    let none_reused = TurnUsage {
        cached_tokens: Some(0),
        ..written(1, 0)
    };
    assert_eq!(none_reused.figure_line(), "41 s \u{b7} 0 tokens reused");
}

/// One token is said in the singular.
#[test]
fn one_token_reused_is_said_in_the_singular() {
    let usage = TurnUsage {
        duration_ms: 500,
        cached_tokens: Some(1),
        ..TurnUsage::default()
    };
    assert_eq!(usage.figure_line(), "0.5 s \u{b7} 1 token reused");
}
