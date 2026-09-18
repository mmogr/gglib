//! Fixtures for the loop guard step's tests: the model name, the settings
//! and store builders, and the history shapes that do or do not trip.
//!
//! A module of its own so that more than one test file can use them —
//! `loop_guard_step_tests.rs` was 292 of the 300 lines a file may be when
//! the step gained a second subject to test.

use std::sync::Arc;

use bytes::Bytes;
use gglib_core::domain::defects::ModelDefectLedger;
use gglib_core::{LoopGuardMode, Settings};
use serde_json::{Value, json};

use super::GuardObservers;
use crate::metrics::ContextMetricsStore;

pub(super) const MODEL: &str = "test-model";

/// Settings whose loop guard runs in `mode`.
///
/// Every test that wants a refusal asks for one: the default is `note`, and
/// the whole point of #1052 is that the guard no longer refuses by default.
pub(super) fn in_mode(mode: LoopGuardMode) -> Settings {
    Settings {
        loop_guard_mode: Some(mode),
        ..Settings::with_defaults()
    }
}

/// A store with a ledger behind it, so a test can read the per-model counts
/// the dashboard reads.
pub(super) fn store() -> (ContextMetricsStore, Arc<ModelDefectLedger>) {
    let ledger = Arc::new(ModelDefectLedger::new());
    (
        ContextMetricsStore::new().with_ledger(Arc::clone(&ledger)),
        ledger,
    )
}

/// Observers with no log behind them: the dashboard's store alone.
pub(super) const fn observing(metrics: &ContextMetricsStore) -> GuardObservers<'_> {
    GuardObservers {
        metrics,
        trips: None,
        session_id: None,
    }
}

pub(super) fn body(history: Vec<Value>) -> Bytes {
    let mut messages = vec![json!({ "role": "system", "content": "be helpful" })];
    messages.extend(history);
    messages.push(json!({ "role": "user", "content": "continue" }));
    Bytes::from(json!({ "model": MODEL, "messages": messages }).to_string())
}

/// The agentic continuation shape: the client executed the calls, appended the
/// results, and asks the model to carry on, so the history ends with a tool
/// result rather than a user turn. [`body`] cannot stand in for it — its
/// trailing `user` turn is chat-shaped and correctly clears the observation.
pub(super) fn agentic_body(history: Vec<Value>) -> Bytes {
    let mut messages = vec![
        json!({ "role": "system", "content": "be helpful" }),
        json!({ "role": "user", "content": "check the file" }),
    ];
    messages.extend(history);
    Bytes::from(json!({ "model": MODEL, "messages": messages }).to_string())
}

pub(super) fn assistant_call(name: &str, args: &str) -> Value {
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
pub(super) fn looping(n: usize) -> Vec<Value> {
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
pub(super) fn repeated_read(n: usize) -> Vec<Value> {
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
pub(super) fn stagnating(n: usize) -> Vec<Value> {
    (0..n)
        .map(|_| json!({ "role": "assistant", "content": "I cannot proceed further." }))
        .collect()
}

pub(super) fn counts(ledger: &ModelDefectLedger) -> gglib_core::domain::defects::ModelDefectCounts {
    ledger.snapshot().get(MODEL).copied().unwrap_or_default()
}
