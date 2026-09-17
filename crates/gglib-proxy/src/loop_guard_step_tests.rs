//! Tests for [`super::run`] — the guard's step, with no server around it.
//!
//! These characterise what the step does today, before the setting in a later
//! commit gives it a third answer. Each one pins something a caller depends
//! on: that the switch-off path scans nothing, that a trip is refused with the
//! 400 an external agentic client already handles, and that the readings the
//! dashboard shows are recorded whether or not the verdict trips.

use std::sync::Arc;

use bytes::Bytes;
use gglib_core::domain::defects::{LoopGuardTrip, ModelDefectLedger};
use gglib_core::{LoopGuardMode, Settings};
use serde_json::{Value, json};

use super::{GuardStep, run};
use crate::metrics::ContextMetricsStore;

const MODEL: &str = "test-model";

/// Settings whose loop guard runs in `mode`.
///
/// Every test that wants a refusal asks for one: the default is `note`, and
/// the whole point of #1052 is that the guard no longer refuses by default.
fn in_mode(mode: LoopGuardMode) -> Settings {
    Settings {
        loop_guard_mode: Some(mode),
        ..Settings::with_defaults()
    }
}

/// A store with a ledger behind it, so a test can read the per-model counts
/// the dashboard reads.
fn store() -> (ContextMetricsStore, Arc<ModelDefectLedger>) {
    let ledger = Arc::new(ModelDefectLedger::new());
    (
        ContextMetricsStore::new().with_ledger(Arc::clone(&ledger)),
        ledger,
    )
}

fn body(history: Vec<Value>) -> Bytes {
    let mut messages = vec![json!({ "role": "system", "content": "be helpful" })];
    messages.extend(history);
    messages.push(json!({ "role": "user", "content": "continue" }));
    Bytes::from(json!({ "model": MODEL, "messages": messages }).to_string())
}

/// The agentic continuation shape: the client executed the calls, appended the
/// results, and asks the model to carry on, so the history ends with a tool
/// result rather than a user turn. [`body`] cannot stand in for it — its
/// trailing `user` turn is chat-shaped and correctly clears the observation.
fn agentic_body(history: Vec<Value>) -> Bytes {
    let mut messages = vec![
        json!({ "role": "system", "content": "be helpful" }),
        json!({ "role": "user", "content": "check the file" }),
    ];
    messages.extend(history);
    Bytes::from(json!({ "model": MODEL, "messages": messages }).to_string())
}

fn assistant_call(name: &str, args: &str) -> Value {
    json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "c1",
            "type": "function",
            "function": { "name": name, "arguments": args }
        }]
    })
}

/// `n` identical batches of a *mutating* tool, each answered the same way. A
/// read-only tool would be held to the far higher observation ceiling.
fn looping(n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|_| {
            vec![
                assistant_call("write_file", r#"{"path":"src/main.rs"}"#),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "1 file changed" }),
            ]
        })
        .collect()
}

/// The same shape with the read-only tool a coding agent repeats: held to the
/// far higher observation ceiling, so it never reaches a verdict.
fn repeated_read(n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|_| {
            vec![
                assistant_call("read_file", r#"{"path":"src/main.rs"}"#),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "fn main() {}" }),
            ]
        })
        .collect()
}

/// `n` identical assistant replies and no tool call anywhere, so the loop
/// detector never sees a batch to count.
fn stagnating(n: usize) -> Vec<Value> {
    (0..n)
        .map(|_| json!({ "role": "assistant", "content": "I cannot proceed further." }))
        .collect()
}

fn counts(ledger: &ModelDefectLedger) -> gglib_core::domain::defects::ModelDefectCounts {
    ledger.snapshot().get(MODEL).copied().unwrap_or_default()
}

#[test]
fn a_guard_switched_off_scans_nothing_and_forwards() {
    let mut settings = Settings::with_defaults();
    settings.proxy_loop_detection = Some(false);
    let (metrics, ledger) = store();

    let step = run(&settings, &body(looping(3)), MODEL, &metrics);

    assert!(matches!(step, GuardStep::Forward));
    // Nothing was scanned, so nothing was recorded: not the refusal snapshot,
    // and not the repeat readings a scan would have produced.
    assert_eq!(metrics.total_requests(), 0);
    assert_eq!(counts(&ledger).loop_guard_trips, 0);
    assert_eq!(counts(&ledger).identical_result_repeats, 0);
}

#[test]
fn a_benign_history_is_forwarded() {
    let (metrics, ledger) = store();

    let step = run(
        &Settings::with_defaults(),
        &body(vec![json!({ "role": "assistant", "content": "done" })]),
        MODEL,
        &metrics,
    );

    assert!(matches!(step, GuardStep::Forward));
    assert_eq!(counts(&ledger).loop_guard_trips, 0);
}

#[test]
fn an_unparseable_body_is_forwarded() {
    let (metrics, _ledger) = store();

    let step = run(
        &Settings::with_defaults(),
        &Bytes::from_static(b"not json at all"),
        MODEL,
        &metrics,
    );

    // Fail-open: this guard is protection, not validation.
    assert!(matches!(step, GuardStep::Forward));
    assert_eq!(metrics.total_requests(), 0);
}

#[test]
fn a_repeated_batch_is_refused_and_counted_as_a_loop() {
    let (metrics, ledger) = store();

    let step = run(
        &in_mode(LoopGuardMode::Refuse),
        &body(looping(3)),
        MODEL,
        &metrics,
    );

    let GuardStep::Refuse(resp) = step else {
        panic!("a repeated batch must not be forwarded");
    };
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    let c = counts(&ledger);
    assert_eq!(c.loop_guard_trips, 1);
    assert_eq!(c.loop_guard_loops, 1);
    assert_eq!(c.loop_guard_stagnations, 0);
    // The refused request records its own snapshot, naming the detector.
    let recent = metrics.recent(1);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].loop_guard_trip, Some(LoopGuardTrip::Loop));
    assert_eq!(recent[0].model_name, MODEL);
}

#[test]
fn a_repeated_reply_is_refused_and_counted_as_stagnation() {
    let (metrics, ledger) = store();

    let step = run(
        &in_mode(LoopGuardMode::Refuse),
        &body(stagnating(6)),
        MODEL,
        &metrics,
    );

    let GuardStep::Refuse(resp) = step else {
        panic!("a stagnating history must not be forwarded");
    };
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    let c = counts(&ledger);
    assert_eq!(c.loop_guard_trips, 1);
    assert_eq!(c.loop_guard_stagnations, 1);
    assert_eq!(c.loop_guard_loops, 0);
    assert_eq!(
        metrics.recent(1)[0].loop_guard_trip,
        Some(LoopGuardTrip::Stagnation)
    );
}

#[tokio::test]
async fn the_refusal_names_the_repeated_signature() {
    let (metrics, _ledger) = store();

    let GuardStep::Refuse(resp) = run(
        &in_mode(LoopGuardMode::Refuse),
        &body(looping(3)),
        MODEL,
        &metrics,
    ) else {
        panic!("a repeated batch must not be forwarded");
    };

    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .expect("refusal body");
    let err: Value = serde_json::from_slice(&bytes).expect("json error body");
    assert_eq!(err["error"]["code"], "loop_detected");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("write_file:"),
        "the refusal names the repeated batch: {message}"
    );
}

#[test]
fn a_repeat_the_verdict_cannot_see_is_still_read() {
    let (metrics, ledger) = store();

    // Two identical read-only batches with identical results: never a verdict
    // at any count, but the reading the dashboard shows is taken anyway.
    let step = run(
        &Settings::with_defaults(),
        &agentic_body(repeated_read(2)),
        MODEL,
        &metrics,
    );

    assert!(matches!(step, GuardStep::Forward));
    let c = counts(&ledger);
    assert_eq!(c.loop_guard_trips, 0);
    assert_eq!(
        c.identical_result_repeats, 1,
        "the diagnosis is recorded for every scanned request, trip or not"
    );
    assert_eq!(
        c.repeats_not_evaluated, 0,
        "the results were joinable, so nothing went unevaluated"
    );
}

#[test]
fn the_default_notes_rather_than_refusing_and_records_nothing_itself() {
    for (history, expected) in [
        (looping(3), LoopGuardTrip::Loop),
        (stagnating(6), LoopGuardTrip::Stagnation),
    ] {
        let (metrics, ledger) = store();

        let GuardStep::Note { note, trip } =
            run(&Settings::with_defaults(), &body(history), MODEL, &metrics)
        else {
            panic!("the default mode is `note`");
        };

        assert_eq!(trip, expected, "the note carries its own detector");
        assert!(
            note.text().contains("[gglib loop guard]"),
            "{}",
            note.text()
        );
        // No snapshot of its own: the request goes on to be forwarded, and
        // the forward records it. Recording here too would count it twice.
        assert_eq!(metrics.total_requests(), 0);
        assert_eq!(counts(&ledger).loop_guard_trips, 0);
    }
}
