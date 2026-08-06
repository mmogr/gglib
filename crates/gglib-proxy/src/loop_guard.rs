//! Pre-dispatch loop/stagnation guard for `/v1/chat/completions`.
//!
//! The built-in agent loop (`gglib-agent`) aborts a run when the model
//! repeats the same tool-call batch or the same response text — but external
//! agentic clients (Cline, Roo Code, Copilot BYOK) run their own loop
//! client-side, where those guards never execute.  A model looping in such a
//! session burns a model swap plus a full generation per stuck turn, and
//! nothing in gglib notices.
//!
//! This module closes that gap **statelessly**: agentic clients replay the
//! full conversation on every request, so the guard reconstructs the
//! detectors' state fresh per request by walking the incoming `messages[]`
//! history through the *same* [`LoopDetector`] and [`StagnationDetector`]
//! the agent path uses (`gglib_core::domain::agent`).  Parity is by
//! construction — there is one detector implementation, not two — and no
//! per-session store, TTL, or eviction is needed.
//!
//! Detection is deliberately **pre-admission**: a tripped guard returns a
//! clean HTTP 400 before any catalog/admission/model-swap cost.  This catches
//! a loop one turn after the agent path's per-iteration check would (the
//! history at turn N shows responses 1..N-1), which caps a runaway session at
//! threshold+1 turns — accepted for a guard whose job is "fail fast and
//! loud", not mid-stream intervention.
//!
//! Parse policy is **fail-open**: this guard is protection, not validation.
//! An unparseable body yields [`LoopGuardVerdict::Pass`] (routing already
//! rejected genuinely invalid JSON), and a tool call whose `arguments` string
//! is not valid JSON is hashed as the raw string rather than erroring — a
//! client sending consistently malformed arguments still gets loop
//! protection and never gets a parse-driven rejection.

use serde::Deserialize;
use serde_json::Value;

use gglib_core::domain::agent::{AgentConfig, LoopDetector, StagnationDetector};
use gglib_core::ports::AgentError;
use gglib_core::{DEFAULT_MAX_STAGNATION_STEPS, Settings, ToolCall};

// =============================================================================
// Configuration
// =============================================================================

/// Thresholds for one request's history scan, resolved from the per-request
/// settings snapshot.
///
/// Loop and observation thresholds come from [`AgentConfig::default`] — the
/// same values the agent path runs with — and the stagnation threshold from
/// the shared persisted `max_stagnation_steps` setting, so the two paths
/// cannot drift.
#[derive(Debug, Clone)]
pub struct LoopGuardConfig {
    max_repeated_batch_steps: usize,
    max_stagnation_steps: usize,
    observation_tools: Vec<String>,
    max_observation_steps: Option<usize>,
}

impl LoopGuardConfig {
    /// Resolve the guard configuration from a settings snapshot.
    ///
    /// Returns `None` when the guard is disabled — either explicitly
    /// (`proxy_loop_detection = Some(false)`) or because the shared agent
    /// defaults disable loop detection entirely.
    pub fn from_settings(settings: &Settings) -> Option<Self> {
        if settings.proxy_loop_detection == Some(false) {
            return None;
        }
        let defaults = AgentConfig::default();
        Some(Self {
            max_repeated_batch_steps: defaults.max_repeated_batch_steps?,
            max_stagnation_steps: settings
                .max_stagnation_steps
                .map_or(DEFAULT_MAX_STAGNATION_STEPS, |v| v as usize),
            observation_tools: defaults.observation_tools,
            max_observation_steps: defaults.max_observation_steps,
        })
    }
}

// =============================================================================
// Verdict
// =============================================================================

/// Outcome of scanning one request's replayed history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopGuardVerdict {
    /// No guard tripped — forward the request.
    Pass,
    /// The same tool-call batch signature repeats beyond the threshold.
    LoopDetected {
        /// The repeated batch signature (`name:hash|name:hash…`).
        signature: String,
    },
    /// The same assistant text repeats beyond the threshold.
    StagnationDetected {
        /// Occurrences seen, including the one that tripped.
        count: usize,
        /// The configured threshold.
        max_steps: usize,
    },
}

// =============================================================================
// Permissive wire types
// =============================================================================
//
// Deliberately NOT `crate::models::ToolCall`: a client replaying history may
// omit `id` or `type` on old messages, and a guard must never 400 a request
// because of a shape quirk in content it is only inspecting.  Every field
// defaults.

#[derive(Deserialize)]
struct HistoryEnvelope {
    #[serde(default)]
    messages: Vec<HistoryMessage>,
}

#[derive(Deserialize)]
struct HistoryMessage {
    #[serde(default)]
    role: String,
    /// String, array-of-parts, or null — inspected via [`extract_text`].
    #[serde(default)]
    content: Value,
    #[serde(default)]
    tool_calls: Vec<WireToolCall>,
}

#[derive(Deserialize)]
struct WireToolCall {
    #[serde(default)]
    id: String,
    #[serde(default)]
    function: WireFunction,
}

#[derive(Deserialize, Default)]
struct WireFunction {
    #[serde(default)]
    name: String,
    /// OpenAI wire format: a JSON-encoded *string*, not an object.
    #[serde(default)]
    arguments: String,
}

// =============================================================================
// History scan
// =============================================================================

/// Walk the request's `messages[]` history through fresh detectors.
///
/// Mirrors `gglib-agent`'s per-iteration `Guards::check` exactly: stagnation
/// records every assistant message's text (the detector itself skips empty
/// text), and the loop detector only sees non-empty tool-call batches.
///
/// Fail-open: an unparseable body returns [`LoopGuardVerdict::Pass`].
pub fn scan_history(body: &[u8], cfg: &LoopGuardConfig) -> LoopGuardVerdict {
    let Ok(envelope) = serde_json::from_slice::<HistoryEnvelope>(body) else {
        return LoopGuardVerdict::Pass;
    };

    let mut stagnation = StagnationDetector::default();
    let mut loops = LoopDetector::default();

    for msg in &envelope.messages {
        if msg.role != "assistant" {
            continue;
        }
        if let Err(e) = stagnation.record(&extract_text(&msg.content), cfg.max_stagnation_steps) {
            return verdict(e);
        }
        if !msg.tool_calls.is_empty() {
            let calls: Vec<ToolCall> = msg.tool_calls.iter().map(to_domain_call).collect();
            if let Err(e) = loops.check(
                &calls,
                cfg.max_repeated_batch_steps,
                &cfg.observation_tools,
                cfg.max_observation_steps,
            ) {
                return verdict(e);
            }
        }
    }

    LoopGuardVerdict::Pass
}

/// Map a detector error onto the guard's verdict.
fn verdict(e: AgentError) -> LoopGuardVerdict {
    match e {
        AgentError::LoopDetected { signature } => LoopGuardVerdict::LoopDetected { signature },
        AgentError::StagnationDetected {
            count, max_steps, ..
        } => LoopGuardVerdict::StagnationDetected { count, max_steps },
        // The detectors return no other variant; treat anything unexpected as
        // a pass rather than inventing a rejection (fail-open).
        _ => LoopGuardVerdict::Pass,
    }
}

/// Extract the assistant-visible text from an OpenAI `content` value.
///
/// `content` may be a plain string, `null` (tool-call-only turns), or an
/// array of typed parts; only `{"type": "text"}` parts contribute.  Anything
/// else (images, unknown part types) yields the empty string, which the
/// stagnation detector ignores.
fn extract_text(content: &Value) -> String {
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

/// Bridge the OpenAI wire tool call (arguments as a JSON string) to the
/// domain [`ToolCall`] (arguments as a [`Value`]) the detectors hash.
///
/// A malformed arguments string falls back to hashing the raw string —
/// identical malformed batches still count as repeats.
fn to_domain_call(call: &WireToolCall) -> ToolCall {
    let arguments = serde_json::from_str::<Value>(&call.function.arguments)
        .unwrap_or_else(|_| Value::String(call.function.arguments.clone()));
    ToolCall {
        id: call.id.clone(),
        name: call.function.name.clone(),
        arguments,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> LoopGuardConfig {
        LoopGuardConfig::from_settings(&Settings::with_defaults()).expect("guard on by default")
    }

    /// Build a request body from raw message values.
    fn body(messages: &[Value]) -> Vec<u8> {
        json!({ "model": "m", "messages": messages })
            .to_string()
            .into_bytes()
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

    fn assistant_text(text: &str) -> Value {
        json!({ "role": "assistant", "content": text })
    }

    // ── Pass cases ───────────────────────────────────────────────────────────

    #[test]
    fn empty_and_first_turn_bodies_pass() {
        assert_eq!(scan_history(&body(&[]), &cfg()), LoopGuardVerdict::Pass);
        let first_turn = body(&[
            json!({ "role": "system", "content": "be helpful" }),
            json!({ "role": "user", "content": "hi" }),
        ]);
        assert_eq!(scan_history(&first_turn, &cfg()), LoopGuardVerdict::Pass);
    }

    #[test]
    fn unparseable_body_fails_open() {
        assert_eq!(
            scan_history(b"not json at all", &cfg()),
            LoopGuardVerdict::Pass
        );
        // messages of the wrong shape entirely — still a pass, not a panic.
        assert_eq!(
            scan_history(br#"{"messages": "nope"}"#, &cfg()),
            LoopGuardVerdict::Pass
        );
    }

    #[test]
    fn non_assistant_roles_are_ignored() {
        // Identical tool results and user messages must never count.
        let msgs: Vec<Value> = (0..10)
            .flat_map(|_| {
                vec![
                    json!({ "role": "user", "content": "same" }),
                    json!({ "role": "tool", "content": "same result", "tool_call_id": "c1" }),
                ]
            })
            .collect();
        assert_eq!(scan_history(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
    }

    #[test]
    fn two_identical_batches_pass_at_default_threshold() {
        let msgs = vec![
            assistant_call("read_file", r#"{"path":"a.rs"}"#),
            assistant_call("read_file", r#"{"path":"a.rs"}"#),
        ];
        assert_eq!(scan_history(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
    }

    #[test]
    fn distinct_arguments_never_trip() {
        let msgs: Vec<Value> = (0..10)
            .map(|i| assistant_call("read_file", &format!(r#"{{"path":"file{i}.rs"}}"#)))
            .collect();
        assert_eq!(scan_history(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
    }

    // ── Loop detection ───────────────────────────────────────────────────────

    #[test]
    fn third_identical_batch_trips_loop() {
        let msgs = vec![
            assistant_call("read_file", r#"{"path":"a.rs"}"#),
            assistant_call("read_file", r#"{"path":"a.rs"}"#),
            assistant_call("read_file", r#"{"path":"a.rs"}"#),
        ];
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ));
    }

    #[test]
    fn shuffled_argument_keys_still_trip() {
        // Same logical arguments, different JSON key order — canonicalized
        // hashing must see them as identical.
        let msgs = vec![
            assistant_call("edit", r#"{"a":1,"b":2}"#),
            assistant_call("edit", r#"{"b":2,"a":1}"#),
            assistant_call("edit", r#"{"a":1,"b":2}"#),
        ];
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ));
    }

    #[test]
    fn batch_signature_ignores_call_order() {
        let pair = |first: &str, second: &str| {
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    { "id": "c1", "function": { "name": first, "arguments": "{}" } },
                    { "id": "c2", "function": { "name": second, "arguments": "{}" } },
                ]
            })
        };
        let msgs = vec![pair("a", "b"), pair("b", "a"), pair("a", "b")];
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ));
    }

    #[test]
    fn malformed_arguments_hash_as_raw_string() {
        // Not valid JSON — must not 400 the request, and identical malformed
        // batches must still count as repeats.
        let msgs: Vec<Value> = (0..3)
            .map(|_| assistant_call("edit", "{not valid json"))
            .collect();
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ));
    }

    #[test]
    fn observation_batches_use_elevated_threshold() {
        // 3 identical snapshot calls would trip the standard threshold (2)
        // but pass under the observation threshold (15)…
        let obs: Vec<Value> = (0..3)
            .map(|_| assistant_call("browser_snapshot", "{}"))
            .collect();
        assert_eq!(scan_history(&body(&obs), &cfg()), LoopGuardVerdict::Pass);

        // …and 16 trips even the elevated threshold.
        let many: Vec<Value> = (0..16)
            .map(|_| assistant_call("browser_snapshot", "{}"))
            .collect();
        assert!(matches!(
            scan_history(&body(&many), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ));
    }

    #[test]
    fn mixed_observation_action_batch_uses_standard_threshold() {
        let mixed = || {
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    { "id": "c1", "function": { "name": "browser_snapshot", "arguments": "{}" } },
                    { "id": "c2", "function": { "name": "do_thing", "arguments": "{}" } },
                ]
            })
        };
        let msgs = vec![mixed(), mixed(), mixed()];
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ));
    }

    // ── Stagnation detection ─────────────────────────────────────────────────

    #[test]
    fn five_identical_texts_pass_then_sixth_trips() {
        let five: Vec<Value> = (0..5).map(|_| assistant_text("I am stuck.")).collect();
        assert_eq!(scan_history(&body(&five), &cfg()), LoopGuardVerdict::Pass);

        let six: Vec<Value> = (0..6).map(|_| assistant_text("I am stuck.")).collect();
        assert_eq!(
            scan_history(&body(&six), &cfg()),
            LoopGuardVerdict::StagnationDetected {
                count: 6,
                max_steps: 5
            }
        );
    }

    #[test]
    fn oscillation_is_counted_session_wide() {
        // A→B→A→B… trips once either text exceeds the threshold, even though
        // no two consecutive responses match.
        let msgs: Vec<Value> = (0..12)
            .map(|i| assistant_text(if i % 2 == 0 { "plan A" } else { "plan B" }))
            .collect();
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::StagnationDetected { .. }
        ));
    }

    #[test]
    fn content_part_arrays_feed_stagnation() {
        let part_msg = || {
            json!({
                "role": "assistant",
                "content": [
                    { "type": "text", "text": "same answer" },
                    { "type": "image_url", "image_url": { "url": "ignored" } },
                ]
            })
        };
        let msgs: Vec<Value> = (0..6).map(|_| part_msg()).collect();
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::StagnationDetected { .. }
        ));
    }

    #[test]
    fn null_content_with_tool_calls_feeds_loop_only() {
        // Tool-call-only turns have null content; the empty text must not
        // accumulate stagnation counts (record() skips empty text), so the
        // verdict is the loop detector's, not a stagnation false positive.
        let msgs = vec![
            assistant_call("t", "{}"),
            assistant_call("t", "{}"),
            assistant_call("t", "{}"),
        ];
        assert!(matches!(
            scan_history(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ));
    }

    // ── Configuration ────────────────────────────────────────────────────────

    #[test]
    fn from_settings_gates_on_proxy_loop_detection() {
        let mut settings = Settings::with_defaults();
        assert!(LoopGuardConfig::from_settings(&settings).is_some());

        settings.proxy_loop_detection = Some(true);
        assert!(LoopGuardConfig::from_settings(&settings).is_some());

        settings.proxy_loop_detection = Some(false);
        assert!(LoopGuardConfig::from_settings(&settings).is_none());
    }

    #[test]
    fn from_settings_honours_persisted_stagnation_threshold() {
        let mut settings = Settings::with_defaults();
        settings.max_stagnation_steps = Some(2);
        let cfg = LoopGuardConfig::from_settings(&settings).expect("enabled");

        let three: Vec<Value> = (0..3).map(|_| assistant_text("stuck")).collect();
        assert_eq!(
            scan_history(&body(&three), &cfg),
            LoopGuardVerdict::StagnationDetected {
                count: 3,
                max_steps: 2
            }
        );
    }
}
