//! The usage frame with and without a context reading: byte for byte what
//! it was for a client told nothing, and the reading inside `usage`, under
//! the names a `turn_usage` event uses, for a client that asked.

use super::tests::{enc, parse_data_frame};
use super::*;

fn usage_event() -> LlmStreamEvent {
    LlmStreamEvent::Usage {
        prompt_tokens: 123,
        completion_tokens: 45,
        total_tokens: 168,
        cached_tokens: Some(100),
    }
}

/// The bytes a client that did not ask for a reading has always been sent.
const PLAIN_USAGE_FRAME: &str = concat!(
    r#"data: {"choices":[],"created":1729000000,"id":"chatcmpl-1","model":"test-model","#,
    r#""object":"chat.completion.chunk","usage":{"completion_tokens":45,"prompt_tokens":123,"#,
    r#""prompt_tokens_details":{"cached_tokens":100},"total_tokens":168}}"#,
    "\n\n",
);

/// An encoder given no reading writes the usage frame byte for byte as it
/// did before a reading existed.
#[test]
fn a_usage_frame_without_a_reading_is_byte_identical() {
    assert_eq!(enc().encode(&usage_event()).unwrap(), PLAIN_USAGE_FRAME);
}

fn usage_of(frame: &str) -> serde_json::Value {
    parse_data_frame(frame)["usage"].clone()
}

/// A client that asked is told both figures inside `usage`, beside the
/// counts it was already sent, in a frame that is otherwise the same.
#[test]
fn a_usage_frame_carries_the_reading_inside_usage() {
    let told = enc().with_reading(Some(ContextReading::new(Some(8192), 3)));
    let out = told.encode(&usage_event()).unwrap();
    let frame = parse_data_frame(&out);
    assert_eq!(
        frame["usage"],
        serde_json::json!({
            "prompt_tokens": 123,
            "completion_tokens": 45,
            "total_tokens": 168,
            "prompt_tokens_details": { "cached_tokens": 100 },
            "context_size": 8192,
            "trimmed_messages": 3,
        })
    );
    assert!(frame["choices"].as_array().is_some_and(Vec::is_empty));
    let mut plain = parse_data_frame(PLAIN_USAGE_FRAME);
    plain["usage"] = frame["usage"].clone();
    assert_eq!(frame, plain, "only `usage` differs");
}

/// Nothing trimmed, or a context nobody knows, is a key left out: never
/// `0`, never `null`, and never a default size.
#[test]
fn nothing_trimmed_is_absent_not_zero() {
    let sized = enc().with_reading(Some(ContextReading::new(Some(4096), 0)));
    let usage = usage_of(&sized.encode(&usage_event()).unwrap());
    assert_eq!(usage["context_size"], 4096);
    assert!(usage.get("trimmed_messages").is_none(), "{usage}");

    let trimmed = enc().with_reading(Some(ContextReading::new(None, 1)));
    let usage = usage_of(&trimmed.encode(&usage_event()).unwrap());
    assert_eq!(usage["trimmed_messages"], 1);
    assert!(usage.get("context_size").is_none(), "{usage}");

    let empty = enc().with_reading(Some(ContextReading::default()));
    assert_eq!(empty.encode(&usage_event()).unwrap(), PLAIN_USAGE_FRAME);
}

/// A reading changes the usage frame and no other.
#[test]
fn a_reading_leaves_every_other_frame_alone() {
    let told = enc().with_reading(Some(ContextReading::new(Some(8192), 3)));
    let events = [
        LlmStreamEvent::TextDelta {
            content: "hello".to_owned(),
        },
        LlmStreamEvent::PromptProgress {
            processed: 2,
            total: 8,
            cached: 1,
            time_ms: 100,
        },
        LlmStreamEvent::Done {
            finish_reason: Some("stop".to_owned()),
        },
    ];
    for event in events {
        assert_eq!(told.encode(&event), enc().encode(&event), "{event:?}");
    }
}

/// The usage frame and a run's `turn_usage` event spell a reading by the
/// same two names, so a client reads either with one decoder.
#[test]
fn the_usage_frame_and_the_turn_usage_event_spell_a_reading_alike() {
    use crate::domain::agent::{AgentEvent, TurnUsage};

    let reading = ContextReading::new(Some(8192), 3);
    let frame = enc().with_reading(Some(reading)).encode(&usage_event());
    let usage = usage_of(&frame.unwrap());
    let event = serde_json::to_value(AgentEvent::TurnUsage(TurnUsage {
        reading,
        ..TurnUsage::default()
    }))
    .unwrap();
    for key in ["context_size", "trimmed_messages"] {
        assert!(usage[key].is_u64(), "{key} in {usage}");
        assert_eq!(usage[key], event[key], "{key}");
    }
}
