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
//! # Why the pass marker exists
//!
//! [`PipelinePass::Repair`] suppresses the pipeline's grammar stage. That
//! stage fires on `tool_choice: "required"` for dialect models and rewrites
//! `tool_choice` to `"none"`, because llama-server rejects a custom grammar
//! combined with `tools`. Left to run on a repair it would convert the
//! re-issue into a request for no tool call at all: a full generation spent,
//! nothing changed, no error anywhere. Pinned by
//! [`the_repair_body_survives_the_pipeline_with_required_intact`].
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
//! [`docs/tool-call-repair.md`]: https://github.com/mmogr/gglib/blob/main/docs/tool-call-repair.md
//! [`the_repair_body_survives_the_pipeline_with_required_intact`]: tests::the_repair_body_survives_the_pipeline_with_required_intact

use bytes::Bytes;
use gglib_core::request_pipeline::{Verdict, validate_tool_calls};
use serde_json::Value;
use tracing::{debug, warn};

/// Environment kill switch, matching the contract of `GGLIB_DISABLE_GRAMMAR`
/// and `GGLIB_DISABLE_AGENTIC_SAMPLING`.
pub const DISABLE_REPAIR_ENV: &str = "GGLIB_DISABLE_TOOL_REPAIR";

/// Whether [`DISABLE_REPAIR_ENV`] is set to a truthy value.
fn repair_disabled_via_env() -> bool {
    std::env::var(DISABLE_REPAIR_ENV).ok().is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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

/// The original request with `tool_choice` forced to `"required"`.
///
/// Everything else is left alone deliberately. The prefix is unchanged so the
/// prompt cache serves the prefill and the re-issue costs decode only; and
/// changing sampling as well would confound which change produced the
/// improvement, when the grammar is what does the work.
fn repair_body(request: &Value) -> Option<Bytes> {
    let mut repaired = request.clone();
    repaired.as_object_mut()?.insert(
        "tool_choice".to_owned(),
        Value::String("required".to_owned()),
    );
    serde_json::to_vec(&repaired).ok().map(Bytes::from)
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
    use gglib_core::request_pipeline::{ModelContext, PipelinePass, SamplingLayers};
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

    /// The interaction that would silently disable the whole feature: stage 6
    /// fires on `tool_choice: "required"` for a dialect model and rewrites it
    /// to `"none"`. On a repair pass it must stand down, or the re-issue asks
    /// for no tool call at all — a full generation spent, nothing changed, no
    /// error anywhere.
    #[test]
    fn the_repair_body_survives_the_pipeline_with_required_intact() {
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

        let mut initial: Value = serde_json::from_slice(&body).unwrap();
        let mut repair: Value = serde_json::from_slice(&body).unwrap();

        gglib_core::request_pipeline::apply(
            &mut initial,
            &ctx,
            &SamplingLayers::default(),
            None,
            PipelinePass::Initial,
        )
        .unwrap();
        gglib_core::request_pipeline::apply(
            &mut repair,
            &ctx,
            &SamplingLayers::default(),
            None,
            PipelinePass::Repair,
        )
        .unwrap();

        assert_eq!(
            initial["tool_choice"], "none",
            "stage 6 rewrites tool_choice on an initial pass — if this ever \
             stops being true the repair pass no longer needs suppressing"
        );
        assert_eq!(
            repair["tool_choice"], "required",
            "a repair pass must reach llama-server still demanding a call"
        );
        assert!(
            repair.get("grammar").is_none(),
            "gglib must not install its own weaker grammar on the repair path"
        );
    }
}
