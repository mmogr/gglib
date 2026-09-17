//! Request bodies the loop-guard integration tests send.
//!
//! One home for them because a test binary cannot import another's private
//! functions, so `integration_loop_guard_tally.rs` had copied three of these
//! verbatim and said so in its own module doc. A fixture is the place a second
//! caller belongs, and it leaves `integration_loop_guard.rs` — frozen at its
//! size by the complexity ratchet — the room its own new cases need.

use serde_json::{Value, json};

/// One assistant turn carrying a single tool call.
pub(crate) fn assistant_call(name: &str, args: &str) -> Value {
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

/// A complete request body: a system turn, `history`, and a trailing user turn
/// — the shape an agentic client replays on every request.
pub(crate) fn chat_body(model: &str, history: Vec<Value>) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": "be helpful" })];
    messages.extend(history);
    messages.push(json!({ "role": "user", "content": "continue" }));
    json!({ "model": model, "stream": false, "messages": messages })
}

/// History with `n` identical tool-call batches (each followed by a tool
/// result, as a real client would replay it).
///
/// Uses a *mutating* tool deliberately. `read_file` and friends are
/// observation tools, whose repeats are held to the far higher
/// `max_observation_steps` ceiling — see
/// `repeated_file_reads_are_not_a_loop` for why that matters.
pub(crate) fn looping_history(n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|_| {
            vec![
                assistant_call("write_file", r#"{"path":"src/main.rs"}"#),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "1 file changed" }),
            ]
        })
        .collect()
}

/// The same shape, but with the read-only tool a coding agent repeats.
pub(crate) fn repeated_read_history(n: usize) -> Vec<Value> {
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
pub(crate) fn stagnating_history(n: usize) -> Vec<Value> {
    (0..n)
        .map(|_| json!({ "role": "assistant", "content": "I cannot proceed further." }))
        .collect()
}
