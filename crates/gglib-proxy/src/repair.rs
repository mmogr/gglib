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
//! — but "ask upstream to use the one it already has". On an `auto` turn the
//! repair request is the original with `tool_choice` forced to `"required"`,
//! which is enough. On a turn gglib's own grammar constrained, upstream's
//! cannot be asked for beside it, so the turn is drawn again under the same
//! grammar instead ([`second_draw_body`]).
//!
//! # Why the repair body bypasses the request pipeline
//!
//! [`repair_body`] and [`second_draw_body`] mutate the already-resolved body
//! and send it. It must never go back to `request_pipeline::apply`.
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
//! by this module's `the_pipeline_would_destroy_a_repair_body_which_is_why_it_bypasses_it`
//! test rather than by a flag nobody sets.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
//! [`docs/tool-call-repair.md`]: https://github.com/mmogr/gglib/blob/main/docs/tool-call-repair.md

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

/// Whether repair runs on a turn, and what constrained it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairTurn {
    /// Whether repair is enabled at all: `Settings.tool_call_repair`.
    pub enabled: bool,
    /// Whether gglib's own grammar constrained this turn: the request
    /// pipeline's stage 6 installed it and rewrote `tool_choice` to `"none"`.
    pub gglib_grammar: bool,
}

impl RepairTurn {
    /// Repair on, on a turn gglib's grammar did not constrain.
    pub const ON: Self = Self {
        enabled: true,
        gglib_grammar: false,
    };
    /// Repair off.
    pub const OFF: Self = Self {
        enabled: false,
        gglib_grammar: false,
    };
}

/// Everything a streaming turn needs to re-issue itself as a tool-call repair.
///
/// Carried into the stream because the decision cannot be made until the call
/// is complete, and by then only the stream knows what was emitted. The
/// re-issue itself is non-streaming.
pub(crate) struct RepairContext {
    /// Cloneable builder for the same upstream endpoint and headers.
    pub req_builder: reqwest::RequestBuilder,
    /// The request body as forwarded upstream, which the repair derives from.
    pub request_body: Bytes,
    /// Whether repair runs, and what constrained the turn.
    pub turn: RepairTurn,
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
        /// The body to re-issue: the original with `tool_choice` forced to
        /// `"required"`, or, on a turn gglib's grammar constrained, the
        /// forwarded body again.
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
/// `turn` says whether repair runs, and whether gglib's own grammar constrained
/// the turn. On such a turn the call is judged all the same, and a violation is
/// drawn again under that grammar.
///
/// Never errors: anything it cannot make sense of yields
/// [`Decision::Forward`], because a repair that misfires costs a generation
/// and replaces a working call with a re-rolled one.
#[must_use]
pub fn decide(request_body: &[u8], response_body: &[u8], turn: RepairTurn) -> Decision {
    if !turn.enabled || repair_disabled_via_env() {
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
    // reproduces it at full cost. gglib's own grammar is the exception: it is
    // weaker than upstream's, and what it let through is judged all the same.
    if !turn.gglib_grammar && !is_auto_tool_choice(&request) {
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
            let body = if turn.gglib_grammar {
                second_draw_body(&request)
            } else {
                repair_body(&request)
            };
            match body {
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

/// The forwarded body again, non-streaming, for a turn gglib's own grammar
/// constrained.
///
/// The grammar and `tool_choice: "none"` stay: llama-server refuses a custom
/// grammar beside any other tool choice, so upstream's own grammar cannot be
/// asked for here. This is a second draw under the same constraint, not a
/// stronger one, and it may choose a different call. `choose` keeps it only if
/// it validates.
fn second_draw_body(request: &Value) -> Option<Bytes> {
    let mut again = request.clone();
    let obj = again.as_object_mut()?;
    obj.insert("stream".to_owned(), Value::Bool(false));
    obj.remove("stream_options");
    serde_json::to_vec(&again).ok().map(Bytes::from)
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
#[path = "repair_tests.rs"]
mod repair_tests;

/// A turn gglib's own grammar constrained.
#[cfg(test)]
#[path = "repair_grammar_tests.rs"]
mod repair_grammar_tests;
