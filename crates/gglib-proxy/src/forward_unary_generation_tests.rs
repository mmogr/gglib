//! What a non-streaming turn is recorded as having generated: the readings
//! of the body the client is answered with, on its snapshot.
//!
//! The upstream, the request and the turn are those of the repair tests.

use std::sync::Arc;

use reqwest::Client;
use serde_json::json;

use super::forward_unary_repair_tests::{
    forwarded_body, refused, run_turn, snapshot, spawn_ordinal_mock,
};
use super::*;

/// An answer of `calls` `read_file` calls, each with these `arguments`, after
/// `reasoning`, that spent `completion_tokens`.
fn answer(
    calls: usize,
    arguments: &str,
    reasoning: &str,
    completion_tokens: u32,
) -> serde_json::Value {
    let call = json!({"id": "call", "type": "function", "function": {
        "name": "read_file", "arguments": arguments
    }});
    json!({
        "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "reasoning_content": reasoning,
            "tool_calls": vec![call; calls]
        }}],
        "usage": {"prompt_tokens": 11, "completion_tokens": completion_tokens, "total_tokens": 99}
    })
}

/// The readings on the one snapshot `metrics` holds.
fn recorded(metrics: &ContextMetricsStore) -> TurnGeneration {
    TurnGeneration::on(&metrics.recent(1)[0])
}

#[tokio::test]
async fn a_unary_turn_records_what_its_answer_carries() {
    let answers = vec![answer(2, "{\"path\":\"a\"}", "résumé", 37)];
    let body = forwarded_body(json!({"tool_choice": "auto"}));
    let (requests, _, metrics) = run_turn(body, answers, None, RepairTurn::OFF).await;

    assert_eq!(requests.len(), 1);
    assert_eq!(
        recorded(&metrics),
        TurnGeneration {
            completion_tokens: Some(37),
            reasoning_chars: 6,
            tool_calls: 2,
            finish_reason: Some("tool_calls".to_owned()),
            generation_ms: None,
        }
    );
}

/// A repaired turn's readings are the second draw's, the body the client is
/// answered with.
#[tokio::test]
async fn a_repaired_unary_turn_records_the_answer_the_client_gets() {
    let answers = vec![
        answer(1, "{\"max_lines\":42}", "first", 5),
        answer(2, "{\"path\":\"a\"}", "the second", 9),
    ];
    let body = forwarded_body(json!({"tool_choice": "auto"}));
    let (requests, _, metrics) = run_turn(body, answers, None, RepairTurn::ON).await;

    assert_eq!(requests.len(), 2, "one original, one repair");
    assert!(metrics.recent(1)[0].tool_repaired);
    assert_eq!(
        recorded(&metrics),
        TurnGeneration {
            completion_tokens: Some(9),
            reasoning_chars: 10,
            tool_calls: 2,
            finish_reason: Some("tool_calls".to_owned()),
            generation_ms: None,
        }
    );
}

/// A turn the upstream refuses is passed through with its body, and nothing
/// is recorded from that body, though it carries all four readings.
#[tokio::test]
async fn a_unary_turn_the_upstream_refuses_records_nothing() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let refusal = refused(&answer(2, "{\"path\":\"a\"}", "résumé", 37));
    let (port, server) = spawn_ordinal_mock(seen, vec![refusal]).await;
    let metrics = ContextMetricsStore::new();
    let seq = metrics.record(snapshot());

    let response = forward_unary(
        Client::new().post(format!("http://127.0.0.1:{port}/v1/chat/completions")),
        forwarded_body(json!({})),
        None,
        &CacheMetricsStore::new(),
        (&metrics, seq),
        RepairTurn::ON,
        crate::unary_body::UNARY_TOTAL_TIMEOUT,
    )
    .await
    .expect("mock upstream reachable");
    server.abort();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let passed_through = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_ne!(
        TurnGeneration::of_unary_body(&passed_through),
        TurnGeneration::default(),
        "the refusal's body has readings to take"
    );
    assert_eq!(recorded(&metrics), TurnGeneration::default());
}
