//! Did the sampling gglib resolved reach llama-server intact?
//!
//! **Tier C — Observation** ([ADR 0001]). Measures whether the rest of the
//! sampling subsystem is doing what it claims. Always on, never gated, and it
//! never changes a decision.
//!
//! # Why this exists
//!
//! Every defect in the sampling hierarchy since June was found the same way:
//! by a human running `curl /slots` by hand. #621 ("measured against a live
//! `/slots` probe"), #745 ("measured on Qwen3.5-4B via `GET /slots`"), and
//! #743/#744 ("all found by running it against a real model rather than by
//! review"). Four times, the same technique, because nothing in gglib ever
//! read back what llama-server actually applied.
//!
//! ADR 0001 is explicit that Tier C "is what makes the other two tiers
//! honest. Without it, 'is this compensation still needed?' is answered by
//! argument." Sampling had no Tier C and was answered by argument for its
//! entire life — which is why it produced roughly a dozen fixes and one
//! outright reversal in two months.
//!
//! The instrument was nearly built already. `slots_poller` has polled
//! `GET /slots` every second for other reasons since #536, and `slots.rs`
//! deliberately discarded the one field that answers this question.
//!
//! # What it can and cannot see
//!
//! Both limits are measured, not assumed — [ADR 0003] finding 7.
//!
//! **It samples; it does not census.** `params` appears only on a slot that
//! is actively processing. An idle slot carries `id`, `n_ctx`, `speculative`
//! and `is_processing` and nothing else. So a comparison happens when a poll
//! lands during a generation, and short requests between polls are never
//! seen. `comparisons` is a count of requests *observed*, never of requests
//! sent, and no rate derived from it is a rate over traffic.
//!
//! **It reads an echo, not the applied chain.** Sending `mirostat: 2`
//! alongside `top_k: 7` leaves `params` reporting `top_k: 7` with a
//! `samplers` array identical to a run without mirostat. So this answers
//! "did what gglib resolved reach llama-server as gglib meant it", and never
//! "what did the model actually sample with". That is the weaker of the two
//! readings and it is still the one that catches #621 and #745, which were
//! both "a value gglib resolved did not arrive intact".
//!
//! A consequence worth stating: a client's own unmodelled sampler can make
//! gglib's values inert without any divergence being reported, because the
//! echo still shows them. Absence of divergence is not proof the model
//! sampled the way gglib intended.
//!
//! # It never acts
//!
//! ADR 0001's static-arbitration rule, and the case is stronger here than
//! for dialects. Feeding a 1 Hz poll back into resolution would make two
//! identical requests decode differently depending on when a poll happened
//! to land, and it would poison the request recorder the rest of this
//! architecture is built to feed. A divergence is logged, counted, and
//! surfaced. Acting on it means someone changing something between runs,
//! with the evidence in hand.
//!
//! # Blind is not agreement
//!
//! ADR 0002 finding 2 named a state ADR 0001 had no vocabulary for: a Tier A
//! module can go *inert* — bypassed, unexercised, unobserved — and look
//! exactly like a module with nothing to do. The same trap applies to a Tier
//! C organ, and harder: if `params` is missing on some build, or the poller
//! never ran, this reports zero divergences, which is indistinguishable from
//! everything agreeing.
//!
//! So [`AuditState`] never collapses to a bare count. `Blind` is a distinct
//! state carrying why, and every surface must render it differently from
//! `Comparing { divergences: 0 }`. This is
//! [`RuntimeCapabilities::unknown`](gglib_core::domain::RuntimeCapabilities::unknown)'s
//! discipline — unknown means nobody knows, never "the feature is absent" —
//! generalised from a capability probe to an observation organ.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md

use gglib_core::domain::ParamSource;
use gglib_core::request_pipeline::SamplingDecision;
use serde::{Deserialize, Serialize};

/// Tolerance for comparing a float that made a round trip through JSON and
/// an `f32`/`f64` narrowing on each side.
///
/// Matches the `assert_param` helper in `request_pipeline::sampling`'s tests,
/// and for the same reason: `0.05f32` widened to `f64` is not `0.05`.
const FLOAT_EPSILON: f64 = 1e-6;

/// The sampler settings llama-server reports for the request in a slot.
///
/// Every field is `Option` with `#[serde(default)]`, following the
/// convention `SlotSnapshot` already established: llama.cpp has changed the
/// *type* of `/slots` fields across versions, and one unexpected shape must
/// degrade that field rather than fail the whole response.
///
/// Only the parameters gglib itself resolves are named. `params` carries 42
/// keys on the pinned build; the rest are not gglib's business and naming
/// them would invent an obligation to keep up with them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SlotParams {
    /// `temperature` as llama-server parsed it.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub temperature: Option<f64>,
    /// `top_p` as llama-server parsed it.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub top_p: Option<f64>,
    /// `top_k` as llama-server parsed it.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub top_k: Option<f64>,
    /// `repeat_penalty` as llama-server parsed it.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub repeat_penalty: Option<f64>,
    /// `presence_penalty` as llama-server parsed it.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub presence_penalty: Option<f64>,
    /// `min_p` as llama-server parsed it.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub min_p: Option<f64>,
    /// `dry_multiplier` as llama-server parsed it.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub dry_multiplier: Option<f64>,
    /// The sampler chain, in the order llama.cpp composes it.
    ///
    /// Not compared against anything — gglib never sets `--samplers`, so
    /// there is no intent to diverge from. Captured because the order is
    /// load-bearing for four simultaneously-sent truncation samplers and was
    /// unstated anywhere in the tree until it was measured.
    #[serde(default)]
    pub samplers: Option<Vec<String>>,
}

/// Read a numeric field as `f64`, degrading a type change to `None`.
///
/// `top_k` is an integer on the wire and the rest are floats; taking them
/// all as `f64` keeps one comparison path rather than two, and the
/// tolerance below is far tighter than any integer gap.
fn tolerant_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|v| v.as_f64()))
}

/// One parameter where what gglib sent and what llama-server reports differ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Divergence {
    /// Wire name of the parameter.
    pub field: &'static str,
    /// What gglib resolved and wrote into the body.
    pub sent: f64,
    /// What llama-server reported for the request in flight.
    pub observed: f64,
    /// The ladder rung the sent value came from, for the log line. A
    /// divergence on a value someone deliberately set reads very differently
    /// from one on a value that fell to the floor.
    pub provenance: String,
}

/// What the audit has actually been able to observe.
///
/// Deliberately not a bare count — see the module docs on `Blind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AuditState {
    /// The poller is running but no in-flight request has been caught yet.
    /// Expected on a quiet server, and on a busy one for a short while.
    NotYetObserved,
    /// Nothing is being compared, and why.
    ///
    /// Rendered distinctly from zero divergences everywhere it is shown. A
    /// silent organ and a healthy one produce the same number and mean
    /// opposite things.
    Blind {
        /// Human-readable cause: slots disabled, no `params` on this build,
        /// upstream unreachable.
        reason: String,
    },
    /// Actively comparing.
    #[serde(rename_all = "camelCase")]
    Comparing {
        /// Requests *observed in flight* — never requests sent. See the
        /// module docs: this instrument samples.
        comparisons: u64,
        /// How many of those disagreed on at least one field.
        divergences: u64,
    },
}

impl AuditState {
    /// Whether this state represents an organ that is actually watching.
    #[must_use]
    pub const fn is_observing(&self) -> bool {
        matches!(self, Self::Comparing { .. })
    }
}

/// Compare what gglib resolved against what llama-server reports.
///
/// # Only fields gglib named
///
/// A parameter gglib deliberately sent nothing for is not a divergence when
/// llama.cpp supplies its own default — that is the design working, and after
/// [ADR 0003]'s deferral it is the normal case for six of seven parameters.
/// So [`ParamSource::Unset`] is skipped rather than compared against zero.
///
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
#[must_use]
pub fn compare(intent: &SamplingDecision, observed: &SlotParams) -> Vec<Divergence> {
    // Nothing reached the wire, so nothing can have diverged from it.
    if !intent.applied {
        return Vec::new();
    }

    let r = &intent.resolved;
    let s = &intent.sources;
    let names = &intent.layer_names;

    let mut out = Vec::new();
    let mut check =
        |field: &'static str, sent: Option<f32>, source: ParamSource, obs: Option<f64>| {
            // `Unset` means gglib named no value: llama.cpp's own default
            // applies and there is no intent to diverge from.
            if source == ParamSource::Unset {
                return;
            }
            let (Some(sent), Some(obs)) = (sent, obs) else {
                return;
            };
            let sent = f64::from(sent);
            if (sent - obs).abs() > FLOAT_EPSILON {
                out.push(Divergence {
                    field,
                    sent,
                    observed: obs,
                    provenance: describe_source(source, names),
                });
            }
        };

    check(
        "temperature",
        r.temperature,
        s.temperature,
        observed.temperature,
    );
    check("top_p", r.top_p, s.top_p, observed.top_p);
    #[allow(clippy::cast_precision_loss)]
    check("top_k", r.top_k.map(|v| v as f32), s.top_k, observed.top_k);
    check(
        "repeat_penalty",
        r.repeat_penalty,
        s.repeat_penalty,
        observed.repeat_penalty,
    );
    check(
        "presence_penalty",
        r.presence_penalty,
        s.presence_penalty,
        observed.presence_penalty,
    );
    check("min_p", r.min_p, s.min_p, observed.min_p);
    check(
        "dry_multiplier",
        r.dry_multiplier,
        s.dry_multiplier,
        observed.dry_multiplier,
    );

    out
}

/// Render a rung for a log line: a name when the value came from a layer,
/// otherwise what kind of fallback supplied it.
fn describe_source(source: ParamSource, names: &[&'static str]) -> String {
    match source {
        ParamSource::Layer(i) => (*names.get(i).unwrap_or(&"?")).to_string(),
        ParamSource::Floor => "floor".to_string(),
        ParamSource::FloorCoupled => "floor (coupled)".to_string(),
        ParamSource::Unset => "unset".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::{FieldSources, InferenceConfig};
    use gglib_core::request_pipeline::{FloorClass, LADDER_RUNGS};

    /// Captured verbatim from a real `/slots` poll during a generation on
    /// the pinned build — `scripts/experiments/sampler_wire_semantics.py`.
    /// The literal shape matters more than the values: it is what the parser
    /// must survive.
    const REAL_SLOT: &str = r#"{
        "temperature": 0.11999999731779099,
        "top_p": 0.949999988079071,
        "top_k": 40,
        "repeat_penalty": 1.0,
        "presence_penalty": 0.0,
        "min_p": 0.05000000074505806,
        "dry_multiplier": 0.0,
        "dry_base": 1.75,
        "mirostat": 0,
        "samplers": ["penalties","dry","top_n_sigma","top_k","typ_p","top_p","min_p","xtc","temperature"]
    }"#;

    fn decision(resolved: InferenceConfig, sources: FieldSources) -> SamplingDecision {
        SamplingDecision {
            resolved,
            sources,
            layer_names: ["cli", "client", "profile", "model", "global", "auto"],
            floor: FloorClass::Default,
            agentic_turn: false,
            agentic_ceiling_applied: None,
            client_fields_rejected: Vec::new(),
            client_fields_discarded: Vec::new(),
            applied: true,
        }
    }

    fn all_from(source: ParamSource) -> FieldSources {
        FieldSources {
            temperature: source,
            top_p: source,
            top_k: source,
            max_tokens: source,
            repeat_penalty: source,
            presence_penalty: source,
            min_p: source,
            dry_multiplier: source,
            dry_base: source,
            dry_allowed_length: source,
            dry_penalty_last_n: source,
        }
    }

    #[test]
    fn a_real_slot_params_payload_parses() {
        let p: SlotParams = serde_json::from_str(REAL_SLOT).expect("real payload parses");
        assert_eq!(p.top_k, Some(40.0));
        assert_eq!(p.samplers.as_ref().unwrap().len(), 9);
        assert_eq!(p.samplers.unwrap().last().unwrap(), "temperature");
    }

    /// The convention `SlotSnapshot` already follows: a field whose *type*
    /// changed degrades to `None` rather than failing the whole response.
    #[test]
    fn a_type_shifted_field_degrades_alone() {
        let p: SlotParams = serde_json::from_str(r#"{"temperature": {"nested": 1}, "top_p": 0.9}"#)
            .expect("one odd field must not fail the parse");
        assert_eq!(p.temperature, None);
        assert_eq!(p.top_p, Some(0.9));
    }

    #[test]
    fn matching_values_do_not_diverge() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let resolved = InferenceConfig {
            temperature: Some(0.12),
            top_k: Some(40),
            min_p: Some(0.05),
            ..Default::default()
        };
        let d = decision(resolved, all_from(ParamSource::Layer(3)));
        assert!(
            compare(&d, &observed).is_empty(),
            "{:?}",
            compare(&d, &observed)
        );
    }

    #[test]
    fn a_changed_value_is_reported_with_its_provenance() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let resolved = InferenceConfig {
            temperature: Some(0.9), // slot says 0.12
            ..Default::default()
        };
        let d = decision(resolved, all_from(ParamSource::Layer(2)));

        let out = compare(&d, &observed);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].field, "temperature");
        assert_eq!(out[0].provenance, "profile");
    }

    /// The case ADR 0003's deferral makes normal: gglib sends nothing and
    /// llama.cpp supplies its own default. That is the design working, and
    /// reporting it would make the counter useless the day deferral ships.
    #[test]
    fn a_value_gglib_never_sent_is_not_a_divergence() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let resolved = InferenceConfig::default(); // nothing resolved
        let d = decision(resolved, all_from(ParamSource::Unset));
        assert!(compare(&d, &observed).is_empty());
    }

    /// A body that was never an object had nothing written to it, so there
    /// is no intent for the wire to disagree with.
    #[test]
    fn an_unapplied_decision_compares_nothing() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let mut d = decision(
            InferenceConfig {
                temperature: Some(0.9),
                ..Default::default()
            },
            all_from(ParamSource::Layer(0)),
        );
        d.applied = false;
        assert!(compare(&d, &observed).is_empty());
    }

    /// Float comparison has to survive `f32` -> JSON -> `f64`. `0.05f32`
    /// widened is 0.05000000074505806, not 0.05.
    #[test]
    fn a_widened_f32_does_not_read_as_a_divergence() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let resolved = InferenceConfig {
            min_p: Some(0.05),
            ..Default::default()
        };
        let d = decision(resolved, all_from(ParamSource::Floor));
        assert!(compare(&d, &observed).is_empty());
    }

    /// The distinction the whole liveness contract exists for.
    #[test]
    fn blind_is_not_the_same_state_as_zero_divergences() {
        let blind = AuditState::Blind {
            reason: "no params on this build".into(),
        };
        let clean = AuditState::Comparing {
            comparisons: 100,
            divergences: 0,
        };
        assert_ne!(blind, clean);
        assert!(!blind.is_observing());
        assert!(clean.is_observing());
        assert!(!AuditState::NotYetObserved.is_observing());
    }

    #[test]
    fn the_ladder_width_matches_the_pipeline() {
        let d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));
        assert_eq!(d.layer_names.len(), LADDER_RUNGS);
    }
}
