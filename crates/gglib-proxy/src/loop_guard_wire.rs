//! The permissive wire view of a replayed chat history.
//!
//! Deliberately NOT `crate::models::ToolCall`: a client replaying history may
//! omit `id` or `type` on old messages, and a guard must never 400 a request
//! because of a shape quirk in content it is only inspecting.
//!
//! ## Every field is a `Value`, and that is the whole design
//!
//! [`crate::loop_guard::scan_history`] fails open on a deserialize error — an unparseable
//! body yields `Pass`. That is right for a body which is not JSON at all, and
//! catastrophic for a body which is merely *shaped* oddly, because the failure
//! is silent and takes the guard down with it for every turn of that
//! conversation. A replayed history only grows, so the offending message comes
//! back on every subsequent request.
//!
//! `#[serde(default)]` does not close this. It fires on a **missing** key,
//! never on a key that is present holding the wrong type. So a typed field is
//! a live switch that any client can flip by accident:
//!
//! - `"tool_calls": null` is what anything serialising an assistant message
//!   from an `Optional[list]` emits — the OpenAI Python SDK's `model_dump()`,
//!   LiteLLM, LangChain. Typed as `Vec<_>` it failed the whole envelope.
//! - `"function": "search"` and `"arguments": {…}` are the same trap one level
//!   down. `arguments` was already fixed for it; `function` was not.
//!
//! So nothing here is typed. Values are read through `as_str` / `as_array` /
//! `get`, and a shape this module does not recognise yields an empty call
//! rather than an error.
//!
//! ## The one exception, stated rather than left to be discovered
//!
//! [`HistoryEnvelope::messages`] is still `Vec<HistoryMessage>`, so a
//! `messages` that is not an array of objects does fail the envelope. That is
//! the intended fail-open: a body whose `messages` is not a list of messages is
//! not a shape quirk in content the guard is inspecting, it is a request the
//! chat endpoint has no way to serve. `unparseable_body_fails_open` pins it.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use gglib_core::ToolCall;
use gglib_core::domain::agent::{batch_results_hash, hash_result_content};

#[derive(Deserialize)]
pub(super) struct HistoryEnvelope {
    #[serde(default)]
    pub(super) messages: Vec<HistoryMessage>,
}

#[derive(Deserialize)]
pub(super) struct HistoryMessage {
    #[serde(default)]
    pub(super) role: Value,
    /// Assistant text is read via [`extract_text`]; on a `role: "tool"`
    /// message the whole value is hashed by [`hash_result_content`] instead.
    #[serde(default)]
    pub(super) content: Value,
    /// The assistant's tool calls, read via [`domain_calls`].
    #[serde(default)]
    pub(super) tool_calls: Value,
    /// Present on `role: "tool"` messages: the id of the call this is the
    /// result of. The join key between the model's request and the
    /// environment's answer, and the reason this struct is no longer
    /// assistant-only.
    #[serde(default)]
    pub(super) tool_call_id: Value,
}

/// The domain calls an assistant turn carries.
///
/// Empty for every shape that is not an array of calls — absent, `null`, an
/// object, a string. All of those mean "this turn made no tool calls" as far
/// as a guard reading someone else's transcript can tell, and the caller
/// already treats an empty batch as a prose turn.
pub(super) fn domain_calls(tool_calls: &Value) -> Vec<ToolCall> {
    tool_calls
        .as_array()
        .map(|calls| calls.iter().map(to_domain_call).collect())
        .unwrap_or_default()
}

/// Bridge one OpenAI wire tool call to the domain [`ToolCall`] the detectors
/// hash.
///
/// A malformed arguments string falls back to hashing the raw string —
/// identical malformed batches still count as repeats.
fn to_domain_call(call: &Value) -> ToolCall {
    let function = call.get("function");
    // A JSON-encoded string is the documented shape, a bare object is the
    // common deviation, and anything else is used as it stands rather than
    // rejected — this runs on content the guard only inspects.
    let arguments = match function.and_then(|f| f.get("arguments")) {
        Some(Value::String(s)) => {
            serde_json::from_str::<Value>(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Null,
    };
    ToolCall {
        id: str_or_empty(call.get("id")),
        name: str_or_empty(function.and_then(|f| f.get("name"))),
        arguments,
    }
}

/// A field's string value, or the empty string for every other shape.
///
/// An unnamed call still participates: it hashes as `""` and two of them look
/// alike, which is the honest reading of a transcript that does not say what
/// was called.
fn str_or_empty(field: Option<&Value>) -> String {
    field.and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// Extract the assistant-visible text from an OpenAI `content` value.
///
/// `content` may be a plain string, `null` (tool-call-only turns), or an
/// array of typed parts; only `{"type": "text"}` parts contribute. Anything
/// else (images, unknown part types) yields the empty string, which the
/// stagnation detector ignores.
pub(super) fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Hash the results answering one assistant turn's tool calls.
///
/// `rest` is the history *after* that assistant message; the answers are the
/// contiguous run of result messages at its head, which bounds the join to
/// this turn and makes repeated synthetic ids harmless. gglib mints those ids
/// itself for dialect models (`DelimitedToolCallParser` restarts at zero on
/// every response), so `call_qwen_0` recurs on every turn of a replayed
/// conversation and a global index would resolve every occurrence of a batch
/// to the same result.
///
/// The windowing is all that lives here. Pairing each call with its own
/// answer, and hashing the pairs, is
/// [`gglib_core::domain::agent::batch_results_hash`] — shared with the agent
/// loop, which has the pairing for free and no window to find.
///
/// `None` when any call is unanswered — a partially-answered batch says
/// nothing about whether work repeated.
pub(super) fn turn_results_hash(calls: &[ToolCall], rest: &[HistoryMessage]) -> Option<u64> {
    let mut answers: HashMap<&str, u64> = HashMap::new();
    for m in rest.iter().take_while(|m| m.role.as_str() == Some("tool")) {
        let Some(id) = m.tool_call_id.as_str() else {
            continue;
        };
        // A window with the same id twice cannot say which call either answer
        // belongs to. Bail rather than keep one: this join's whole contract is
        // that it reports nothing when it cannot attribute an answer.
        if answers
            .insert(id, hash_result_content(&m.content))
            .is_some()
        {
            return None;
        }
    }

    // The same, from the other side. Two calls sharing an id would both resolve
    // to the single answer present, so a half-answered batch would report as
    // fully joined and take a strike from an answer that never existed — or, if
    // that one answer moved, manufacture a rescue. The agent path joins
    // positionally and can do neither, so this is also where the two paths
    // would drift.
    let mut seen: HashMap<&str, ()> = HashMap::with_capacity(calls.len());
    for c in calls {
        if seen.insert(c.id.as_str(), ()).is_some() {
            return None;
        }
    }

    let per_call: Vec<Option<u64>> = calls
        .iter()
        .map(|c| answers.get(c.id.as_str()).copied())
        .collect();
    batch_results_hash(calls, &per_call)
}

#[cfg(test)]
#[path = "loop_guard_wire_tests.rs"]
mod tests;
