//! Re-issue a turn whose tool call did not match the advertised schema.
//!
//! **Tier B — Policy** (see [ADR 0001] and [`docs/tool-call-repair.md`]). The
//! detection half is [`gglib_core::request_pipeline::validate`], which is pure
//! and lives in core. This module is the half that needs a second upstream
//! request, so it lives where requests are made.
//!
//! # The mechanism
//!
//! Measured on `b10327` ([ADR 0002], findings 4-5): where `tool_choice:
//! "auto"` is unconstrained, a 3B model puts `max_lines` as the string `"42"`
//! on 26 of 30 calls; where `tool_choice: "required"` installs llama.cpp's
//! own schema-derived grammar, the same model is conformant 30 of 30.
//!
//! So repair is not "originate a grammar" — that work was dropped in ADR 0002
//! — but "ask upstream to use the one it already has". The repair request is
//! the original with `tool_choice` forced to `"required"`, which is enough.
//!
//! # Why the repair body bypasses the request pipeline
//!
//! [`repair_body`] mutates the already-resolved body and sends it. It must
//! never hand that body back to `request_pipeline::apply`.
//!
//! The pipeline's grammar stage fires on `tool_choice: "required"` for
//! dialect models and rewrites `tool_choice` to `"none"`, because
//! llama-server rejects a custom grammar combined with `tools`. Run on a
//! repair it would convert the re-issue into a request for no tool call at
//! all: a full generation spent, nothing changed, no error anywhere.
//!
//! A `PipelinePass` marker used to encode this, suppressing the stage on a
//! repair pass. It was removed because the case never arose — the repair path
//! does not call `apply`, so every caller passed `Initial` and the other
//! branch was unreachable. The hazard is real but structural, and is pinned
//! by [`the_pipeline_would_destroy_a_repair_body_which_is_why_it_bypasses_it`]
//! rather than by a flag nobody sets.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
//! [`docs/tool-call-repair.md`]: https://github.com/mmogr/gglib/blob/main/docs/tool-call-repair.md
//! [`the_pipeline_would_destroy_a_repair_body_which_is_why_it_bypasses_it`]: tests::the_pipeline_would_destroy_a_repair_body_which_is_why_it_bypasses_it

use bytes::Bytes;
use gglib_core::LlmStreamEvent;
use gglib_core::request_pipeline::{Verdict, validate_tool_calls};
use serde_json::{Value, json};
use tracing::{debug, warn};

/// Environment kill switch, matching the contract of `GGLIB_DISABLE_GRAMMAR`
/// and `GGLIB_DISABLE_AGENTIC_SAMPLING`.
pub const DISABLE_REPAIR_ENV: &str = "GGLIB_DISABLE_TOOL_REPAIR";

/// Whether [`DISABLE_REPAIR_ENV`] is set to a truthy value.
fn repair_disabled_via_env() -> bool {
    gglib_core::debug_switches::enabled(DISABLE_REPAIR_ENV)
}

/// Why a response was not repaired, for the record.
///
/// A repair that does not happen is as worth explaining as one that does —
/// "conformant" and "we declined to look" are very different facts about a
/// model, and a counter that conflates them cannot inform the per-model
/// grammar-presence question ADR 0002 leaves open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skipped {
    /// Every emitted call matched its schema.
    Conformant,
    /// No tools advertised, or no calls emitted.
    NotApplicable,
    /// The schema uses constructs the validator does not implement.
    Unvalidatable,
    /// The client already asked for `required`, so the grammar was already
    /// installed and re-issuing changes nothing.
    AlreadyConstrained,
    /// Turned off by settings or [`DISABLE_REPAIR_ENV`].
    Disabled,
    /// The response or request body could not be read as JSON.
    Unreadable,
}

/// What to do about a response's tool calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Forward the response unchanged.
    Forward(Skipped),
    /// Re-issue with this body, then forward whichever result is better.
    Reissue {
        /// The original request body with `tool_choice` forced to `"required"`.
        body: Bytes,
        /// Rendered violations, for logging and the record.
        violations: Vec<String>,
    },
}

/// Decide whether `response_body` warrants a repair re-issue.
///
/// `request_body` is the body **as forwarded upstream** — it is the thing the
/// re-issue is derived from, and it already carries the shaping the original
/// request received.
///
/// Never errors: anything it cannot make sense of yields
/// [`Decision::Forward`], because a repair that misfires costs a generation
/// and replaces a working call with a re-rolled one.
#[must_use]
pub fn decide(request_body: &[u8], response_body: &[u8], enabled: bool) -> Decision {
    if !enabled || repair_disabled_via_env() {
        return Decision::Forward(Skipped::Disabled);
    }

    let (Ok(request), Ok(response)) = (
        serde_json::from_slice::<Value>(request_body),
        serde_json::from_slice::<Value>(response_body),
    ) else {
        return Decision::Forward(Skipped::Unreadable);
    };

    // Already-constrained requests are checked before validation: when the
    // client asked for `required`, upstream's grammar was already installed,
    // so a violation is something that grammar does not cover and re-issuing
    // reproduces it at full cost.
    if !is_auto_tool_choice(&request) {
        return Decision::Forward(Skipped::AlreadyConstrained);
    }

    let verdict = validate_tool_calls(request.get("tools"), first_tool_calls(&response));

    match verdict {
        Verdict::Valid => Decision::Forward(Skipped::Conformant),
        Verdict::NotApplicable => Decision::Forward(Skipped::NotApplicable),
        Verdict::Unvalidatable(reason) => {
            debug!(reason, "tool call not validatable; forwarding unchanged");
            Decision::Forward(Skipped::Unvalidatable)
        }
        Verdict::Invalid(violations) => {
            let rendered: Vec<String> = violations.iter().map(ToString::to_string).collect();
            match repair_body(&request) {
                Some(body) => Decision::Reissue {
                    body,
                    violations: rendered,
                },
                None => Decision::Forward(Skipped::Unreadable),
            }
        }
    }
}

/// Whether the request left the tool choice to the model.
///
/// Absent counts as `auto`, which is what the `OpenAI` contract says and what
/// every agentic client relies on.
fn is_auto_tool_choice(request: &Value) -> bool {
    match request.get("tool_choice") {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s == "auto",
        _ => false,
    }
}

/// The first choice's `tool_calls`, if any.
fn first_tool_calls(response: &Value) -> Option<&Value> {
    response
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("tool_calls")
}

/// The original request with `tool_choice` forced to `"required"`, sent
/// non-streaming.
///
/// Sampling and messages are left alone deliberately: the prefix is unchanged
/// so the prompt cache serves the prefill and the re-issue costs decode only,
/// and changing sampling too would confound which change produced the
/// improvement when the grammar is what does the work.
///
/// `stream` is forced **off**. A repair cannot be judged until the call is
/// complete, so streaming the re-issue would buy no latency while requiring a
/// second SSE pipeline to run inside the first — with its own decoder,
/// normalizer, encoder and `[DONE]` bookkeeping. A buffered body is parsed
/// once and synthesized into events by [`synthesize_tool_call_events`], which
/// keeps every frame the client sees flowing through the one `SseEncoder` that
/// has been encoding this turn all along.
fn repair_body(request: &Value) -> Option<Bytes> {
    let mut repaired = request.clone();
    let obj = repaired.as_object_mut()?;
    obj.insert(
        "tool_choice".to_owned(),
        Value::String("required".to_owned()),
    );
    obj.insert("stream".to_owned(), Value::Bool(false));
    obj.remove("stream_options");
    serde_json::to_vec(&repaired).ok().map(Bytes::from)
}

/// Assembles streamed [`LlmStreamEvent::ToolCallDelta`] fragments into the
/// `OpenAI` `tool_calls` shape the validator reads.
///
/// The stream carries `id` and `name` only on the first delta for an index and
/// `arguments` in fragments, so reconstructing the call is the only way to know
/// what was actually emitted.
#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    calls: Vec<(Option<String>, Option<String>, String)>,
}

impl ToolCallAccumulator {
    /// Fold one delta in.
    pub fn push(&mut self, index: usize, id: Option<&str>, name: Option<&str>, args: Option<&str>) {
        if self.calls.len() <= index {
            self.calls.resize(index + 1, (None, None, String::new()));
        }
        let slot = &mut self.calls[index];
        if let Some(id) = id {
            slot.0 = Some(id.to_owned());
        }
        if let Some(name) = name {
            slot.1 = Some(name.to_owned());
        }
        if let Some(args) = args {
            slot.2.push_str(args);
        }
    }

    /// Whether any delta has been seen.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// The assembled calls in `OpenAI` non-streaming shape, for validation.
    #[must_use]
    pub fn to_tool_calls(&self) -> Value {
        Value::Array(
            self.calls
                .iter()
                .map(|(id, name, args)| {
                    json!({
                        "id": id.clone().unwrap_or_default(),
                        "type": "function",
                        "function": {
                            "name": name.clone().unwrap_or_default(),
                            "arguments": args,
                        }
                    })
                })
                .collect(),
        )
    }
}

/// Turn a buffered repair response's `tool_calls` into stream events.
///
/// One event per call rather than per fragment: the client reassembles deltas
/// by index either way, and a single complete delta cannot be interleaved
/// wrongly or truncated mid-arguments.
#[must_use]
pub fn synthesize_tool_call_events(response_body: &[u8]) -> Vec<LlmStreamEvent> {
    let Ok(response) = serde_json::from_slice::<Value>(response_body) else {
        return Vec::new();
    };
    let Some(calls) = first_tool_calls(&response).and_then(Value::as_array) else {
        return Vec::new();
    };

    calls
        .iter()
        .enumerate()
        .map(|(index, call)| LlmStreamEvent::ToolCallDelta {
            index,
            id: call
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            name: call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            arguments: call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        })
        .collect()
}

/// Choose between the original response and a repair attempt's.
///
/// The repaired response wins only if it actually validates. A repair that is
/// still wrong is discarded and the original forwarded — the same fail-open
/// discipline truncation and the loop guard already apply, and for the same
/// reason: a protection that can leave the client worse off than its absence
/// is not a protection.
#[must_use]
pub fn choose(request_body: &[u8], original: Bytes, repaired: Bytes) -> (Bytes, bool) {
    let (Ok(request), Ok(response)) = (
        serde_json::from_slice::<Value>(request_body),
        serde_json::from_slice::<Value>(&repaired),
    ) else {
        return (original, false);
    };

    match validate_tool_calls(request.get("tools"), first_tool_calls(&response)) {
        Verdict::Valid => (repaired, true),
        other => {
            warn!(
                verdict = ?std::mem::discriminant(&other),
                violations = other.violations().len(),
                "tool-call repair did not produce a conformant call; forwarding the original"
            );
            (original, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::{DialectSpec, ModelCapabilities};
    use gglib_core::request_pipeline::{ModelContext, SamplingLayers};
    use serde_json::json;

    fn request(tool_choice: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "model": "m",
            "messages": [{"role": "user", "content": "read it"}],
            "tool_choice": tool_choice,
            "tools": [{
                "type": "function",
                "function": {
                    "name": "read_file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "max_lines": {"type": "integer"}
                        },
                        "required": ["path"]
                    }
                }
            }]
        }))
        .unwrap()
    }

    fn response(arguments: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "type": "function",
                        "function": {"name": "read_file", "arguments": arguments}
                    }]
                }
            }]
        }))
        .unwrap()
    }

    /// The measured Llama 3.2 failure: an integer field carrying a string.
    #[test]
    fn a_schema_violation_on_the_auto_path_is_reissued() {
        let d = decide(
            &request(json!("auto")),
            &response(r#"{"path":"a","max_lines":"42"}"#),
            true,
        );

        let Decision::Reissue { body, violations } = d else {
            panic!("expected a re-issue, got {d:?}");
        };
        assert!(violations[0].contains("max_lines"), "{violations:?}");

        let parsed: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["tool_choice"], "required");
        assert_eq!(
            parsed["messages"],
            request_value()["messages"],
            "prefix unchanged"
        );
    }

    fn request_value() -> Value {
        serde_json::from_slice(&request(json!("auto"))).unwrap()
    }

    // ── Accumulator and synthesis ────────────────────────────────────────

    /// Streamed deltas carry `id`/`name` only on the first fragment and split
    /// `arguments` arbitrarily, so reassembly is the only way to know what was
    /// emitted.
    #[test]
    fn fragmented_deltas_reassemble_into_one_call() {
        let mut acc = ToolCallAccumulator::default();
        acc.push(0, Some("call_1"), Some("read_file"), Some(r#"{"path":"#));
        acc.push(0, None, None, Some(r#""a","max_lines":"#));
        acc.push(0, None, None, Some("42}"));

        let calls = acc.to_tool_calls();
        assert_eq!(calls[0]["function"]["name"], "read_file");
        assert_eq!(calls[0]["id"], "call_1");
        assert_eq!(
            calls[0]["function"]["arguments"],
            r#"{"path":"a","max_lines":42}"#
        );
    }

    /// Parallel calls arrive interleaved by index, not in sequence.
    #[test]
    fn interleaved_indices_stay_separate() {
        let mut acc = ToolCallAccumulator::default();
        acc.push(0, Some("a"), Some("read_file"), Some(r#"{"path":"#));
        acc.push(1, Some("b"), Some("read_file"), Some(r#"{"path":"#));
        acc.push(1, None, None, Some(r#""two"}"#));
        acc.push(0, None, None, Some(r#""one"}"#));

        let calls = acc.to_tool_calls();
        assert_eq!(calls.as_array().unwrap().len(), 2);
        assert_eq!(calls[0]["function"]["arguments"], r#"{"path":"one"}"#);
        assert_eq!(calls[1]["function"]["arguments"], r#"{"path":"two"}"#);
    }

    /// An index arriving before its predecessors must not panic or misplace.
    #[test]
    fn an_out_of_order_first_index_does_not_panic() {
        let mut acc = ToolCallAccumulator::default();
        acc.push(2, Some("c"), Some("read_file"), Some("{}"));

        let calls = acc.to_tool_calls();
        assert_eq!(calls.as_array().unwrap().len(), 3);
        assert_eq!(calls[2]["id"], "c");
    }

    #[test]
    fn an_empty_accumulator_is_empty() {
        assert!(ToolCallAccumulator::default().is_empty());
    }

    /// The assembled shape must be exactly what the validator reads, or the
    /// hold-back would validate something the client never receives.
    #[test]
    fn the_assembled_shape_validates_like_a_real_response() {
        let mut acc = ToolCallAccumulator::default();
        acc.push(
            0,
            Some("x"),
            Some("read_file"),
            Some(r#"{"path":"a","max_lines":"42"}"#),
        );

        let wrapped = json!({"choices": [{"message": {"tool_calls": acc.to_tool_calls()}}]});
        let bytes = serde_json::to_vec(&wrapped).unwrap();

        assert!(matches!(
            decide(&request(json!("auto")), &bytes, true),
            Decision::Reissue { .. }
        ));
    }

    /// One event per call, not per fragment: a complete delta cannot be
    /// truncated mid-arguments or interleaved wrongly on the wire.
    #[test]
    fn synthesis_emits_one_complete_event_per_call() {
        let body = response(r#"{"path":"a","max_lines":42}"#);
        let events = synthesize_tool_call_events(&body);

        assert_eq!(events.len(), 1);
        let LlmStreamEvent::ToolCallDelta {
            index,
            name,
            arguments,
            ..
        } = &events[0]
        else {
            panic!("expected a ToolCallDelta");
        };
        assert_eq!(*index, 0);
        assert_eq!(name.as_deref(), Some("read_file"));
        assert_eq!(arguments.as_deref(), Some(r#"{"path":"a","max_lines":42}"#));
    }

    #[test]
    fn synthesis_of_an_unreadable_body_yields_nothing() {
        assert!(synthesize_tool_call_events(b"garbage").is_empty());
    }

    /// The repair must go out non-streaming, or a second SSE pipeline would
    /// have to run inside the first.
    #[test]
    fn the_repair_request_is_non_streaming() {
        let Decision::Reissue { body, .. } = decide(
            &request(json!("auto")),
            &response(r#"{"path":"a","max_lines":"42"}"#),
            true,
        ) else {
            panic!("expected a re-issue");
        };

        let parsed: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["stream"], false);
        assert!(parsed.get("stream_options").is_none());
    }

    #[test]
    fn a_conformant_call_is_forwarded() {
        assert_eq!(
            decide(&request(json!("auto")), &response(r#"{"path":"a"}"#), true),
            Decision::Forward(Skipped::Conformant)
        );
    }

    /// Re-issuing with `required` when the client already asked for it would
    /// reproduce the same failure at full cost.
    #[test]
    fn an_already_required_request_is_not_reissued() {
        assert_eq!(
            decide(
                &request(json!("required")),
                &response(r#"{"path":"a","max_lines":"42"}"#),
                true
            ),
            Decision::Forward(Skipped::AlreadyConstrained)
        );
    }

    /// Absent `tool_choice` is `auto` per the OpenAI contract, and is what
    /// most clients actually send.
    #[test]
    fn an_absent_tool_choice_counts_as_auto() {
        let d = decide(
            &request(json!(null)),
            &response(r#"{"path":"a","max_lines":"42"}"#),
            true,
        );
        assert!(matches!(d, Decision::Reissue { .. }));
    }

    #[test]
    fn disabling_repair_forwards_everything() {
        assert_eq!(
            decide(
                &request(json!("auto")),
                &response(r#"{"path":"a","max_lines":"42"}"#),
                false
            ),
            Decision::Forward(Skipped::Disabled)
        );
    }

    #[test]
    fn an_unreadable_body_is_forwarded() {
        assert_eq!(
            decide(b"not json", &response(r#"{"path":"a"}"#), true),
            Decision::Forward(Skipped::Unreadable)
        );
    }

    #[test]
    fn a_response_without_tool_calls_is_not_applicable() {
        let plain = serde_json::to_vec(&json!({
            "choices": [{"message": {"role": "assistant", "content": "hello"}}]
        }))
        .unwrap();

        assert_eq!(
            decide(&request(json!("auto")), &plain, true),
            Decision::Forward(Skipped::NotApplicable)
        );
    }

    #[test]
    fn a_conformant_repair_replaces_the_original() {
        let original = Bytes::from(response(r#"{"path":"a","max_lines":"42"}"#));
        let repaired = Bytes::from(response(r#"{"path":"a","max_lines":42}"#));

        let (chosen, did_repair) = choose(&request(json!("auto")), original, repaired.clone());

        assert!(did_repair);
        assert_eq!(chosen, repaired);
    }

    /// Fail-open: a repair that is still wrong must never be forwarded in
    /// place of the original.
    #[test]
    fn a_still_invalid_repair_is_discarded() {
        let original = Bytes::from(response(r#"{"path":"a","max_lines":"42"}"#));
        let repaired = Bytes::from(response(r#"{"path":"a","max_lines":"still bad"}"#));

        let (chosen, did_repair) = choose(&request(json!("auto")), original.clone(), repaired);

        assert!(!did_repair);
        assert_eq!(chosen, original);
    }

    #[test]
    fn an_unreadable_repair_is_discarded() {
        let original = Bytes::from(response(r#"{"path":"a"}"#));
        let (chosen, did_repair) = choose(
            &request(json!("auto")),
            original.clone(),
            Bytes::from_static(b"garbage"),
        );

        assert!(!did_repair);
        assert_eq!(chosen, original);
    }

    /// Why a repair body must never be sent back through the request
    /// pipeline.
    ///
    /// Stage 6 fires on `tool_choice: "required"` for a dialect model,
    /// installs gglib's own grammar and rewrites `tool_choice` to `"none"` —
    /// llama-server rejects a custom grammar alongside `tools`. Applied to a
    /// repair that silently converts the re-issue into a request for no tool
    /// call at all: a full generation spent, nothing changed, no error
    /// anywhere.
    ///
    /// `repair_body` avoids this by construction — it mutates the
    /// already-resolved body and sends it, never calling `apply`. This test
    /// demonstrates the damage that bypass prevents, so the reason survives
    /// as something executable rather than as a comment nobody re-derives.
    #[test]
    fn the_pipeline_would_destroy_a_repair_body_which_is_why_it_bypasses_it() {
        let ctx = ModelContext {
            capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS,
            catalog_resolved: true,
            dialect: Some(DialectSpec::qwen_xml()),
            ..ModelContext::passthrough()
        };

        let Decision::Reissue { body, .. } = decide(
            &request(json!("auto")),
            &response(r#"{"path":"a","max_lines":"42"}"#),
            true,
        ) else {
            panic!("expected a re-issue");
        };

        // What `repair_body` actually produces, and sends as-is.
        let as_sent: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            as_sent["tool_choice"], "required",
            "the re-issue must reach llama-server still demanding a call"
        );
        assert!(
            as_sent.get("grammar").is_none(),
            "gglib must not install its own weaker grammar on the repair path"
        );

        // The same body put through the pipeline — the thing that must not
        // happen. If this ever stops rewriting `tool_choice`, the bypass is
        // no longer load-bearing and this test should be revisited.
        let mut through_pipeline: Value = serde_json::from_slice(&body).unwrap();
        gglib_core::request_pipeline::apply(
            &mut through_pipeline,
            &ctx,
            &SamplingLayers::default(),
            None,
        )
        .unwrap();
        assert_eq!(
            through_pipeline["tool_choice"], "none",
            "stage 6 would ask for no tool call at all — hence the bypass"
        );
    }
}
