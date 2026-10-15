//! A bypassed dialect on both chat paths: the request gets no grammar, the
//! answer's markup reaches the client as text, and the drift alarm counts it.
//!
//! The dialect's markers are its own, in no other dialect's list, so only a
//! scan for the model's spec can find them.

use std::time::Duration;

use bytes::Bytes;
use futures_util::stream;
use gglib_core::LlmStreamEvent;
use gglib_core::domain::{DialectSpec, ModelCapabilities};
use gglib_core::request_pipeline::{ModelContext, SamplingLayers, apply};
use serde_json::{Value, json};

use super::TurnDialect;
use crate::client_send::ClientSender;
use crate::forward::drain_events;
use crate::unary_body::normalize_non_streaming_body;

/// A dialect whose markers only its own spec names.
fn spec() -> DialectSpec {
    DialectSpec {
        id: "own-markers".to_owned(),
        tool_open: "<<call>>".to_owned(),
        tool_close: "<</call>>".to_owned(),
        ..DialectSpec::qwen_xml()
    }
}

/// One call, written as the model writes it.
const MARKUP: &str = r#"<<call>>{"name":"read_file","arguments":{"path":"a"}}<</call>>"#;

/// A catalog model with [`spec`], asked for a call it must make.
fn demanded() -> (ModelContext, Value) {
    let context = ModelContext {
        capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS,
        dialect: Some(spec()),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let body = json!({
        "model": "m",
        "messages": [{"role": "user", "content": "read a"}],
        "tools": [{"type": "function", "function": {"name": "read_file", "parameters": {}}}],
        "tool_choice": "required",
    });
    (context, body)
}

/// The request as the pipeline shapes it for `dialect`.
fn shaped(dialect: &TurnDialect) -> Value {
    let (context, mut body) = demanded();
    apply(
        &mut body,
        &dialect.shaping_context(&context),
        &SamplingLayers::default(),
        None,
    )
    .expect("the pipeline applies");
    body
}

#[test]
fn a_parsed_turn_gets_the_grammar_a_demanded_call_needs() {
    let body = shaped(&TurnDialect::parsed(Some(spec())));

    assert!(body.get("grammar").is_some(), "{body}");
    assert_eq!(body["tool_choice"], "none");
}

/// Stage 6 is off with the parser: its grammar's markup would reach the
/// client as text, under a `tool_choice` it rewrote to `"none"`.
#[test]
fn a_bypassed_turn_gets_no_grammar_and_keeps_its_tool_choice() {
    let body = shaped(&TurnDialect::bypassed(Some(spec())));

    assert!(body.get("grammar").is_none(), "{body}");
    assert_eq!(body["tool_choice"], "required");
}

/// A non-streaming answer whose call is written as text.
fn answer() -> Bytes {
    let body = json!({
        "choices": [{"index": 0, "message": {"role": "assistant", "content": MARKUP}}]
    });
    Bytes::from(body.to_string())
}

#[test]
fn a_parsed_unary_answer_has_its_call_and_no_residue() {
    let (body, residue) =
        normalize_non_streaming_body(answer(), &TurnDialect::parsed(Some(spec())));

    let body: Value = serde_json::from_slice(&body).expect("json");
    let message = &body["choices"][0]["message"];
    assert_eq!(message["tool_calls"][0]["function"]["name"], "read_file");
    assert_eq!(residue, None);
}

#[test]
fn a_bypassed_unary_answer_keeps_its_markup_and_the_alarm_counts_it() {
    let (body, residue) =
        normalize_non_streaming_body(answer(), &TurnDialect::bypassed(Some(spec())));

    let body: Value = serde_json::from_slice(&body).expect("json");
    let message = &body["choices"][0]["message"];
    assert_eq!(message["content"], MARKUP);
    assert!(message.get("tool_calls").is_none(), "{message}");
    assert_eq!(residue.as_deref(), Some("<<call>>"));
}

/// Drain the call, streamed as text, read as `dialect` says.
async fn drained(dialect: TurnDialect) -> crate::forward::StreamOutcome {
    let events = stream::iter([
        Ok(LlmStreamEvent::TextDelta {
            content: MARKUP.to_owned(),
        }),
        Ok(LlmStreamEvent::Done {
            finish_reason: Some("stop".to_owned()),
        }),
    ]);
    let registry = std::sync::Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register("m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);
    let reader = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let outcome = drain_events(
        events,
        "m".to_owned(),
        dialect,
        ClientSender::new(tx, Duration::from_secs(5)),
        &connection,
        None,
        None,
    )
    .await;
    reader.await.expect("the client task does not panic");
    outcome
}

#[tokio::test]
async fn a_parsed_stream_has_its_call_and_no_residue() {
    let outcome = drained(TurnDialect::parsed(Some(spec()))).await;

    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(outcome.dialect_residue, None);
}

#[tokio::test]
async fn a_bypassed_stream_keeps_its_markup_and_the_alarm_counts_it() {
    let outcome = drained(TurnDialect::bypassed(Some(spec()))).await;

    assert_eq!(outcome.tool_calls, 0);
    assert_eq!(outcome.dialect_residue.as_deref(), Some("<<call>>"));
}
