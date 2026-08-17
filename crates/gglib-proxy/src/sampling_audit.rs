//! Did the sampling gglib resolved reach llama-server intact?
//!
//! **Tier C — Observation** ([ADR 0001]). Measures whether the rest of the
//! sampling subsystem is doing what it claims. Always on, never gated, and it
//! never changes a decision.
//!
//! # Why this exists
//!
//! [ADR 0003] deletes six of the seven values gglib force-wrote into every
//! request, because all six were measured to be exactly llama.cpp's own
//! defaults on the pinned build. That deferral is safe **only while the
//! build is pinned**: if a bump moves an upstream default gglib now defers
//! to, nothing else in the system is in a position to notice. This is what
//! notices.
//!
//! Secondarily it catches transmission faults — a value resolved but lost to
//! serialization or overwritten downstream — and a client's own unmodelled
//! sampler changing what the server does.
//!
//! # What it does *not* catch
//!
//! Stated up front because an earlier version of this doc claimed the
//! opposite, and the claim survived into an accepted ADR.
//!
//! **It cannot see a resolution bug.** #621 resolved `presence_penalty: 1.5`
//! from the wrong layer and sent 1.5; #745 resolved `dry_multiplier: 0.0`
//! after the coupling rule discarded 0.8, and sent 0.0. In both, intent and
//! wire agreed perfectly. Comparing them reports nothing. gglib decided the
//! wrong thing and transmitted it faithfully.
//!
//! Those belong to the other half of the arc — the `Displaced` provenance
//! variant and property tests over the fold, which ask "is what we resolved
//! what the user asked for?". This asks "is what we resolved what the server
//! got?". Complementary questions; neither instrument substitutes for the
//! other, and conflating them is how this module's purpose got overstated in
//! the first place.
//!
//! [ADR 0001]'s point still stands, though: Tier C "is what makes the other
//! two tiers honest. Without it, 'is this compensation still needed?' is
//! answered by argument." Sampling had no Tier C at all, and produced
//! roughly a dozen fixes and one outright reversal in two months.
//!
//! The instrument was nearly built already. `slots_poller` has polled
//! `GET /slots` every second for other reasons since #536, and `slots.rs`
//! deliberately discarded the one field that answers this question.
//!
//! # Coverage, and the two limits on it
//!
//! Both measured, not assumed — [ADR 0003] finding 7.
//!
//! **It samples; it does not census, and it is biased toward long turns.**
//! `params` appears only on a slot that is actively processing; an idle slot
//! omits it. So coverage depends on how long a turn runs relative to the
//! poll interval. Measured at 1 Hz on the pinned build: ~5 s turns were
//! caught 12/12, ~0.6 s turns 6/12. `comparisons` counts requests
//! *observed*, never requests sent, and no rate derived from it is a rate
//! over traffic.
//!
//! The upside of the same fact: an idle slot reports *nothing* rather than
//! the previous request's values, so there are no stale readings to defend
//! against and no ring of recent observations is needed.
//!
//! **It reads an echo, not the applied chain.** Sending `mirostat: 2`
//! alongside `top_k: 7` leaves `params` reporting `top_k: 7` with a
//! `samplers` array identical to a run without mirostat. So a client's own
//! unmodelled sampler can render gglib's values inert with no divergence
//! reported, because the echo still shows them. Absence of divergence is not
//! proof the model sampled the way gglib intended.
//!
//! # Ambiguity, and why it abstains
//!
//! gglib never sees llama-server's `id_task` in a chat-completions response,
//! so a slot cannot be joined to the request that filled it. With four slots
//! processing at once — the norm under any parallel client — an observation
//! cannot be attributed.
//!
//! Rather than invent a correlation, [`compare_poll`] **abstains**: it
//! compares only when every intent in flight agrees on the fields being
//! compared, and otherwise counts `skipped_ambiguous`.
//!
//! That costs almost nothing in practice, which was measured rather than
//! assumed. With `trust_client_sampling` off — the default — all compared
//! fields come from the ladder rather than the client, so concurrent
//! requests against one model and profile resolve identically. Four
//! concurrent turns with identical resolution produced 0 ambiguous polls out
//! of 10; four whose parameters genuinely differed produced 9 out of 9. The
//! abstention fires exactly where guessing would have been wrong.
//!
//! Note "identical" means identical **on the compared fields**. Comparing
//! whole decisions would abstain constantly, because `max_tokens` is
//! client-authoritative and varies per request while nothing else does.
//!
//! It abstains for a second reason too: **a busy slot gglib has no request to
//! explain did not come through this proxy.** llama-server is reachable
//! directly, and `llama::args::sampling` records that as the one population the
//! deleted launch flags ever served. Comparing such a slot against a gglib
//! intent would invent a divergence and attach a confident provenance to it.
//! So when there are more busy slots than intents in flight, the whole poll is
//! skipped rather than the surplus — slots arrive in no order, so comparing a
//! subset would make the false positive rarer without making it less wrong.
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
//! # Two fields this instrument can never see, and will not pretend to
//!
//! [`compare`] covers **seven** readback fields — the sampler values in
//! [`SlotParams`] — plus `seed`. Neither reasoning control can ever join them,
//! and this is a structural fact about llama-server rather than a gap waiting
//! to be filled:
//!
//! - `reasoning_effort` becomes a chat-template kwarg consumed at render time.
//!   No sampler ever holds it, so no sampler echo can report it.
//! - `reasoning_budget_tokens` **is** parsed into `params.sampling`, and
//!   `task_params::to_json` (`tools/server/server-task.cpp:32-147`) serialises
//!   no `reasoning_budget_*` field in either of its branches. 49 `/slots`
//!   params captured mid-generation carried neither field, and neither appears
//!   in `/props.default_generation_settings.params` either.
//!   `server-schema.cpp:383` names the key, but that is the request-*parse*
//!   table, not an echo.
//!
//! Measured on the pinned build — [ADR 0007] finding 7a, which corrects that
//! ADR's own earlier claim that the budget was observable. So **adding either
//! field to [`SlotParams`] would create a column that can only ever be `None`**,
//! and a permanently-`None` observation read as agreement is the exact failure
//! the section below is about. `no_reasoning_field_may_join_the_readback` fails
//! the build if one is added.
//!
//! What replaces the comparison is [`crate::audit_records`]: gglib's own record
//! of what it resolved, carried with the reason nothing corroborates it.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use gglib_core::domain::{
    InferenceConfig, ModelSamplingDefaults, ParamSource, ReasoningEffort, SamplingOverride,
};
use gglib_core::request_pipeline::{SamplingDecision, SuppressedEffort};
use serde::{Deserialize, Serialize};

use crate::audit_records::{
    BudgetRung, ClientFieldNameTally, ClientFieldNames, EffortRung, EffortSupportState,
    ReasoningReadback, ResolvedReasoning, WIRE_BLIND_REASON,
};

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
    /// The RNG seed llama-server applied to this slot.
    ///
    /// Reported as `4294967295` (`u32::MAX`) when the server drew its own,
    /// which is what gglib sending no seed produces — so that value is the
    /// observation matching an unseeded intent, not a divergence.
    ///
    /// Read as `f64` like every other field here so one tolerant parser covers
    /// the struct; `u32::MAX` is exactly representable, so the round trip is
    /// lossless across the whole range a seed can take.
    #[serde(default, deserialize_with = "tolerant_f64")]
    pub seed: Option<f64>,
    /// The sampler chain, in the order llama.cpp composes it.
    ///
    /// Not compared against anything — gglib never sets `--samplers`, so
    /// there is no intent to diverge from. Captured because the order is
    /// load-bearing for four simultaneously-sent truncation samplers and was
    /// unstated anywhere in the tree until it was measured.
    #[serde(default)]
    pub samplers: Option<Vec<String>>,
}

impl SlotParams {
    /// Look one parameter up by its wire name.
    ///
    /// For callers iterating a table of field names — [`crate::props`]'s
    /// baseline check — rather than reading fields it knows statically.
    /// An unknown name is `None`, indistinguishable from a field this build
    /// did not report, which is correct: both mean "no reading".
    #[must_use]
    pub fn get(&self, field: &str) -> Option<f64> {
        match field {
            "temperature" => self.temperature,
            "top_p" => self.top_p,
            "top_k" => self.top_k,
            "repeat_penalty" => self.repeat_penalty,
            "presence_penalty" => self.presence_penalty,
            "min_p" => self.min_p,
            "dry_multiplier" => self.dry_multiplier,
            _ => None,
        }
    }
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
///
/// Serialize-only, like every type on the dashboard contract
/// ([`crate::dashboard::DashboardSnapshot`]): `field` is a `&'static str`
/// because the names are gglib's own, and nothing reads these back into Rust.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
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
pub(crate) fn compare(intent: &SamplingDecision, observed: &SlotParams) -> Vec<Divergence> {
    // Nothing reached the wire, so nothing can have diverged from it.
    if !intent.applied {
        return Vec::new();
    }

    let r = &intent.resolved;
    let s = &intent.sources;
    let names = &intent.layer_names;

    let mut out = Vec::new();

    // Checked before `check` exists, because that closure holds a mutable
    // borrow of `out` for the rest of the function.
    //
    // Seed has no `FieldSources` entry — it is request-scoped and never
    // resolved from a rung — so it could not use `check` anyway, which gates on
    // provenance. The gate here is simply whether gglib named one: an unseeded
    // request has no intent to diverge from, and llama-server reporting its own
    // random `u32::MAX` is then the expected observation rather than a fault.
    //
    // Worth comparing at all because "did my seed actually land?" is the
    // premise every reproducibility claim rests on. It is also the one field
    // where a silent drop would be invisible in the output — a benchmark would
    // simply read the resulting variance as signal.
    if let (Some(sent), Some(obs)) = (r.seed, observed.seed)
        && (f64::from(sent) - obs).abs() > FLOAT_EPSILON
    {
        out.push(Divergence {
            field: "seed",
            sent: f64::from(sent),
            observed: obs,
            provenance: "request".to_string(),
        });
    }

    // `reasoning_effort` and `reasoning_budget_tokens` are absent from this
    // function on purpose, and no future `SlotParams` field will fix that.
    // Neither is echoed anywhere: effort becomes a Jinja kwarg consumed at
    // render time, and `task_params::to_json` serialises no
    // `reasoning_budget_*` field in either branch — measured against the
    // pinned build, 49 slot params captured mid-generation, neither present
    // (ADR 0007 finding 7a, which corrects that ADR's own earlier claim that
    // the budget was observable; the request-*parse* table at
    // `server-schema.cpp:383` is not an echo). Both are permanently Blind, and
    // their `FieldSources` entries are the only account of the decision.
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

/// The comparable fingerprint of an intent: the seven fields
/// [`compare`] checks, and nothing else.
///
/// Two intents are interchangeable for audit purposes when these agree, even
/// if the requests differed in `max_tokens` or anything else. Using the whole
/// [`SamplingDecision`] here would abstain on almost every poll, because
/// `max_tokens` is client-authoritative and varies request to request while
/// the ladder-supplied fields do not.
fn comparable_key(d: &SamplingDecision) -> [Option<u64>; 7] {
    let r = &d.resolved;
    // Bit patterns rather than floats: this is an equality check between two
    // values gglib itself produced from the same code path, not a tolerance
    // question. `to_bits` also sidesteps `f32` not being `Eq`.
    let b = |v: Option<f32>| v.map(|x| u64::from(x.to_bits()));
    [
        b(r.temperature),
        b(r.top_p),
        r.top_k.map(|v| v as u64),
        b(r.repeat_penalty),
        b(r.presence_penalty),
        b(r.min_p),
        b(r.dry_multiplier),
    ]
}

/// What one poll of `/slots` yielded.
#[derive(Debug, Clone, PartialEq)]
pub struct PollOutcome {
    /// Slots compared against an intent.
    pub comparisons: u64,
    /// Of those, how many disagreed on at least one field.
    pub divergences: u64,
    /// Observations skipped because the intents in flight disagreed, so no
    /// observation could be attributed to one of them.
    pub skipped_ambiguous: u64,
    /// Every field-level disagreement found, for logging.
    pub found: Vec<Divergence>,
}

/// Compare one `/slots` poll against the intents currently in flight.
///
/// `intents` is the set of recently-issued [`SamplingDecision`]s for the
/// running model; `observed` is the `params` of every slot that was
/// processing. Abstains rather than guessing — see the module docs.
///
/// # Two ways an observation cannot be attributed
///
/// The intents in flight disagree on a compared field, so which one an
/// observation belongs to changes the answer. That is the case the module docs
/// measure the cost of.
///
/// Or there are **more busy slots than gglib has requests to explain them**, in
/// which case at least one is somebody else's — a client curling llama-server
/// directly, which is precisely the population `llama::args::sampling` records
/// the deleted launch flags as having served. Comparing an arbitrary slot
/// against a gglib intent would then manufacture a `Divergence` carrying a
/// confident provenance string, on the instrument whose only value is that it
/// is worth believing when it fires.
///
/// Both abstain over the whole poll rather than over the surplus. Comparing
/// `min(observed, intents)` of them would be worse than useless: the slots
/// arrive in no particular order, so the ones compared would be an arbitrary
/// subset and the false positive would merely become less frequent and no less
/// wrong.
#[must_use]
pub(crate) fn compare_poll(intents: &[SamplingDecision], observed: &[SlotParams]) -> PollOutcome {
    let mut out = PollOutcome {
        comparisons: 0,
        divergences: 0,
        skipped_ambiguous: 0,
        found: Vec::new(),
    };
    if observed.is_empty() {
        return out;
    }

    // No intent recorded yet for this model — nothing to compare against.
    // Not ambiguity; just nothing to say.
    let Some(first) = intents.first() else {
        return out;
    };

    // More busy slots than gglib can account for: at least one belongs to
    // something that did not come through this proxy.
    if observed.len() > intents.len() {
        out.skipped_ambiguous = observed.len() as u64;
        return out;
    }

    let key = comparable_key(first);
    if intents.iter().any(|i| comparable_key(i) != key) {
        out.skipped_ambiguous = observed.len() as u64;
        return out;
    }

    for params in observed {
        out.comparisons += 1;
        let found = compare(first, params);
        if !found.is_empty() {
            out.divergences += 1;
            out.found.extend(found);
        }
    }
    out
}

// =============================================================================
// SamplingAuditStore
// =============================================================================

/// How many recent divergences are kept for display.
///
/// Small on purpose. A divergence is meant to be rare and investigated; a long
/// scrollback would imply it is a stream to be monitored, which is the wrong
/// posture for a signal that should never fire.
const MAX_RECENT_DIVERGENCES: usize = 20;

/// Everything the sampling readback has observed since the proxy started.
///
/// Written by the `/slots` poller and the request path, read by the dashboard.
/// `std::sync::Mutex` around the two small collections, following
/// [`crate::metrics::ContextMetricsStore`]'s convention: every critical
/// section is a push and a conditional pop, with no `.await` inside.
#[derive(Default)]
pub struct SamplingAuditStore {
    comparisons: AtomicU64,
    divergences: AtomicU64,
    skipped_ambiguous: AtomicU64,
    /// Client sampling fields that could not be read as sent.
    client_fields_rejected: AtomicU64,
    /// Client sampling fields dropped by the trust gate. Expected to be large
    /// in the default configuration — `trust_client_sampling` is off, so every
    /// client-supplied field is discarded by design. Counted so that "gglib is
    /// ignoring my temperature" is answerable from the dashboard instead of
    /// from the source.
    client_fields_discarded: AtomicU64,
    /// Why the organ cannot see, when it cannot. `None` means it can.
    blind: Mutex<Option<String>>,
    recent: Mutex<VecDeque<Divergence>>,
    /// The most recent `/props` baseline reading, one per model launch.
    baseline: Mutex<crate::props::BaselineState>,
    /// The running model's template-capability self-report (ADR 0007), read
    /// from the same `/props` body as the baseline and held with the same
    /// once-per-launch discipline.
    ///
    /// Storage only: the gate reads the *catalog row's* copy via
    /// `ModelContext`, not this one, and the dashboard for this live reading
    /// lands with the readback work. The gate's *output* is on the snapshot.
    template_caps: Mutex<gglib_core::domain::TemplateCapsState>,
    /// The running model's name and what its GGUF publishes, set once per
    /// launch by the poller. `None` until the first poll names a model; the
    /// inner `Option` is `None` for a model with no metadata read.
    model_sampling: Mutex<Option<(String, Option<ModelSamplingDefaults>)>>,
    /// The published-vs-sent comparison, refreshed from each resolved intent.
    published: Mutex<PublishedOverrides>,
    /// Requests whose resolved `reasoning_effort` stage 5b threw away.
    effort_suppressed: AtomicU64,
    /// The most recent one, with its rung already named. See
    /// [`EffortSuppressions`].
    latest_effort_suppressed: Mutex<Option<SuppressedEffortRecord>>,
    /// What the most recent request resolved for the two reasoning controls.
    ///
    /// The counterpart to [`Self::recent`] for the two fields that have no
    /// wire side: there is nothing to compare, so the record *is* the
    /// observation. `None` until a request has been resolved — which is a
    /// different fact from a request that named neither control, and
    /// [`ResolvedReasoning`] keeps them apart.
    latest_reasoning: Mutex<Option<ResolvedReasoning>>,
    /// Which client field names were dropped, and how often each.
    ///
    /// Beside [`Self::client_fields_discarded`] rather than replacing it: the
    /// count is the total including anything past the tally's bound, and the
    /// names are what make it actionable. See [`crate::audit_records`].
    client_field_names: ClientFieldNameTally,
}

impl SamplingAuditStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what the request pipeline resolved, for the counters that do not
    /// need a wire observation to be meaningful.
    ///
    /// # Why `effort_suppressed` is a parameter and not an inference
    ///
    /// It could have been read off `decision.sources.reasoning_effort` — stage
    /// 5b rewrites that entry to
    /// [`SuppressedByTemplate`](ParamSource::SuppressedByTemplate) — and that
    /// would have spared the caller a field. It would also have thrown away the
    /// two things worth keeping: `decision.resolved.reasoning_effort` is `None`
    /// by then, and the rung that asked for the level has been overwritten by
    /// the suppression marker. [`SuppressedEffort`] exists precisely because
    /// those are unrecoverable from the decision alone.
    ///
    /// Passing it explicitly also closes the hole this parameter was added to
    /// fix: the proxy built [`PipelineReport::effort_suppressed`] on every
    /// request and dropped it on the floor at both shaping sites, so the level
    /// and the rung were computed, logged once at `debug!`, and then lost. A
    /// caller can no longer forget it without the compiler saying so.
    ///
    /// [`PipelineReport::effort_suppressed`]: gglib_core::request_pipeline::PipelineReport::effort_suppressed
    pub fn record_intent(
        &self,
        decision: &SamplingDecision,
        effort_suppressed: Option<&SuppressedEffort>,
    ) {
        if let Some(suppressed) = effort_suppressed {
            self.effort_suppressed.fetch_add(1, Ordering::Relaxed);
            *self
                .latest_effort_suppressed
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(SuppressedEffortRecord {
                level: suppressed.level,
                source: describe_source(suppressed.source, &decision.layer_names),
            });
        }

        *self
            .latest_reasoning
            .lock()
            .unwrap_or_else(|e| e.into_inner()) =
            Some(resolved_reasoning(decision, effort_suppressed));

        let rejected = decision.client_fields_rejected.len() as u64;
        let discarded = decision.client_fields_discarded.len() as u64;
        if rejected > 0 {
            self.client_fields_rejected
                .fetch_add(rejected, Ordering::Relaxed);
        }
        if discarded > 0 {
            self.client_fields_discarded
                .fetch_add(discarded, Ordering::Relaxed);
        }
        // The names behind those two counts. A count cannot answer "why did my
        // reasoning_effort do nothing?"; the name can.
        self.client_field_names.record(
            &decision.client_fields_discarded,
            &decision.client_fields_rejected,
        );

        // Compare what this request resolved against what the model published.
        // Skipped entirely until a poll has named the running model — a
        // comparison against defaults gglib does not have yet would report
        // "nothing overridden" about a model it has not read.
        let model = self
            .model_sampling
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|(_, defaults)| *defaults);
        if let Some(model) = model {
            let fields = compare_published(&model, &decision.resolved);
            let mut guard = self.published.lock().unwrap_or_else(|e| e.into_inner());
            guard.intents = guard.intents.saturating_add(1);
            guard.fields = fields;
        }
    }

    /// Fold one poll's outcome into the totals.
    ///
    /// A poll that compared something clears any [`AuditState::Blind`] latch:
    /// whatever was wrong before, the organ is demonstrably seeing now.
    pub fn record_poll(&self, outcome: &PollOutcome) {
        self.comparisons
            .fetch_add(outcome.comparisons, Ordering::Relaxed);
        self.divergences
            .fetch_add(outcome.divergences, Ordering::Relaxed);
        self.skipped_ambiguous
            .fetch_add(outcome.skipped_ambiguous, Ordering::Relaxed);

        if outcome.comparisons > 0 {
            *self.blind.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        if !outcome.found.is_empty() {
            let mut guard = self.recent.lock().unwrap_or_else(|e| e.into_inner());
            for d in &outcome.found {
                guard.push_back(d.clone());
                if guard.len() > MAX_RECENT_DIVERGENCES {
                    guard.pop_front();
                }
            }
        }
    }

    /// Latch the organ as unable to observe, with the reason.
    ///
    /// Idempotent, and deliberately not cleared here — only a successful
    /// comparison clears it, so a transient recovery that never actually
    /// compares anything cannot make the dashboard claim sight it does not
    /// have.
    pub fn mark_blind(&self, reason: impl Into<String>) {
        *self.blind.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason.into());
    }

    /// Store the baseline reading for the currently running model.
    ///
    /// Overwrites rather than merges: a model swap must not leave the previous
    /// model's table on display, and
    /// [`Unreadable`](crate::props::BaselineState::Unreadable) is a more
    /// honest thing to show in its place than a stale `Read`.
    pub fn set_baseline(&self, state: crate::props::BaselineState) {
        *self.baseline.lock().unwrap_or_else(|e| e.into_inner()) = state;
    }

    /// Store the template-caps reading for the currently running model.
    ///
    /// Overwrites, for [`Self::set_baseline`]'s reason: a model swap must not
    /// leave the previous template's self-report on display, and `Unreadable`
    /// is a more honest thing to hold than a stale `Read`.
    pub fn set_template_caps(&self, state: gglib_core::domain::TemplateCapsState) {
        *self.template_caps.lock().unwrap_or_else(|e| e.into_inner()) = state;
    }

    /// The stored template-caps reading. `NotYetRead` until the poller's
    /// first `/props` read completes for the running model.
    #[must_use]
    pub fn template_caps(&self) -> gglib_core::domain::TemplateCapsState {
        self.template_caps
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Record what the running model's GGUF publishes.
    ///
    /// Called beside [`Self::set_baseline`], from the poller that already reads
    /// it. Resets the comparison rather than merging: a model swap must not
    /// leave the previous model's published values on display, and the intent
    /// count must not carry across either — it counts requests resolved
    /// *against this model*.
    ///
    /// Keyed on the model **name**, not on the defaults themselves. Two models
    /// that publish nothing compare equal, so a value-keyed reset would carry
    /// one model's intent count into the next and report a comparison that had
    /// not happened. The poller retries this until `/props` reads, so it must
    /// also be idempotent within one launch — which name-keying gives.
    pub fn set_model_sampling(&self, model_name: &str, model: Option<ModelSamplingDefaults>) {
        let mut current = self
            .model_sampling
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if current.as_ref().is_some_and(|(name, _)| name == model_name) {
            return;
        }
        *current = Some((model_name.to_owned(), model));
        *self.published.lock().unwrap_or_else(|e| e.into_inner()) = PublishedOverrides::default();
    }

    /// What the organ can currently say about itself.
    #[must_use]
    pub fn state(&self) -> AuditState {
        if let Some(reason) = self
            .blind
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return AuditState::Blind {
                reason: reason.clone(),
            };
        }
        let comparisons = self.comparisons.load(Ordering::Relaxed);
        if comparisons == 0 {
            return AuditState::NotYetObserved;
        }
        AuditState::Comparing {
            comparisons,
            divergences: self.divergences.load(Ordering::Relaxed),
        }
    }

    /// The full reading, for the dashboard.
    #[must_use]
    pub fn snapshot(&self) -> SamplingAuditSnapshot {
        SamplingAuditSnapshot {
            state: self.state(),
            skipped_ambiguous: self.skipped_ambiguous.load(Ordering::Relaxed),
            client_fields_rejected: self.client_fields_rejected.load(Ordering::Relaxed),
            client_fields_discarded: self.client_fields_discarded.load(Ordering::Relaxed),
            recent_divergences: self
                .recent
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .cloned()
                .collect(),
            baseline: self
                .baseline
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            published: self
                .published
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            effort_suppressed: EffortSuppressions {
                requests: self.effort_suppressed.load(Ordering::Relaxed),
                latest: self
                    .latest_effort_suppressed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
            },
            reasoning: ReasoningReadback {
                effort_support: EffortSupportState::of(&self.template_caps()),
                latest: self
                    .latest_reasoning
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
                wire_blind_reason: WIRE_BLIND_REASON,
            },
            client_field_names: self.client_field_names.snapshot(),
        }
    }
}

/// What one request resolved for the two reasoning controls, with each rung
/// already named.
///
/// Free-standing rather than a method, because it needs
/// [`SamplingDecision::layer_names`] and the suppression together — and the
/// suppression is the only place a suppressed level survives at all. Stage 5b
/// rewrites `sources.reasoning_effort` to
/// [`SuppressedByTemplate`](ParamSource::SuppressedByTemplate) and clears
/// `resolved.reasoning_effort`, so a record built from the decision alone would
/// report that gglib asked for nothing — which is precisely the erasure
/// [`SuppressedEffort`] exists to prevent.
fn resolved_reasoning(
    decision: &SamplingDecision,
    effort_suppressed: Option<&SuppressedEffort>,
) -> ResolvedReasoning {
    let names = &decision.layer_names;
    let effort = match (effort_suppressed, decision.resolved.reasoning_effort) {
        (Some(suppressed), _) => Some(EffortRung {
            level: suppressed.level,
            source: describe_source(suppressed.source, names),
            suppressed: true,
        }),
        (None, Some(level)) => Some(EffortRung {
            level,
            source: describe_source(decision.sources.reasoning_effort, names),
            suppressed: false,
        }),
        (None, None) => None,
    };
    ResolvedReasoning {
        effort,
        budget: decision
            .resolved
            .reasoning_budget_tokens
            .map(|tokens| BudgetRung {
                tokens,
                source: describe_source(decision.sources.reasoning_budget_tokens, names),
            }),
    }
}

/// One resolved `reasoning_effort` stage 5b deleted, in a form a surface can
/// render.
///
/// [`SuppressedEffort`] carries the rung as a [`ParamSource`], whose
/// `Layer(i)` indexes a ladder no reader of a JSON snapshot has. Resolved to
/// the rung's name here, at the one point where
/// [`SamplingDecision::layer_names`] is still in hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuppressedEffortRecord {
    /// The level the ladder resolved and llama-server never saw.
    pub level: ReasoningEffort,
    /// The rung that asked for it — `profile`, `model`, `global`, `cli`.
    pub source: String,
}

/// What the effort gate has thrown away since this proxy started.
///
/// # Why the audit is where this lands
///
/// Every other consumer of a request's sampling can re-derive its subject from
/// something. This one cannot: a suppressed `reasoning_effort` leaves **no
/// trace anywhere else in the system**. It is deleted from the body before the
/// request is sent, and neither reasoning control is echoed by `/slots.params`
/// or `/props` (ADR 0007 finding 7a), so the readback that catches every other
/// transmission fault is permanently blind to this one. That makes it Tier C's
/// business by elimination — it is an observation about gglib's own behaviour
/// that nothing else is positioned to make — and it is the reason the record is
/// kept at all rather than left to a `debug!` line an operator has to have been
/// running at the time to see.
///
/// # Zero is not "nothing was suppressed"
///
/// [`AuditState`]'s rule again. `requests: 0` is the honest reading on a model
/// whose template reads the variable, on a model nobody has probed, and on a
/// proxy that has served no request at all — three states with different
/// meanings. A surface rendering this must say what it is beside, not present
/// the count alone.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct EffortSuppressions {
    /// Requests on which a resolved level was deleted before sending.
    pub requests: u64,
    /// The most recent suppression, or `None` if there has been none.
    ///
    /// The most recent rather than a list, for the reason
    /// [`PublishedOverrideField`] gives: with `trust_client_sampling` off every
    /// request against one model and profile resolves identically, so a list
    /// would be the same entry repeated. [`Self::requests`] is what says how
    /// often.
    pub latest: Option<SuppressedEffortRecord>,
}

/// Compare one resolved config against what the model published.
///
/// Reads gglib's side from [`InferenceConfig::to_openai_json_patch`] — the very
/// map the request pipeline merges into the body — rather than from the struct
/// fields. A parameter missing from it is one gglib names nowhere, which is
/// exactly the condition under which the model's own value reaches the sampler.
/// Deriving it any other way would let this report and the request disagree.
fn compare_published(
    model: &ModelSamplingDefaults,
    resolved: &InferenceConfig,
) -> Vec<PublishedOverrideField> {
    let patch = resolved.to_openai_json_patch();
    model
        .compare_all(|field| patch.get(field).and_then(serde_json::Value::as_f64))
        .into_iter()
        .filter_map(|(field, verdict)| {
            let (key, state) = match verdict {
                SamplingOverride::NotPublished => return None,
                SamplingOverride::Deferred { key, published } => {
                    (key, PublishedOverrideState::Deferred { published })
                }
                SamplingOverride::Restated { key, published } => {
                    (key, PublishedOverrideState::Restated { published })
                }
                SamplingOverride::Overridden {
                    key,
                    published,
                    sending,
                } => (
                    key,
                    PublishedOverrideState::Overridden { published, sending },
                ),
                SamplingOverride::Unreadable { key, .. } => {
                    (key, PublishedOverrideState::Unreadable)
                }
            };
            Some(PublishedOverrideField { field, key, state })
        })
        .collect()
}

/// Serializable view of [`SamplingAuditStore`], carried on the dashboard
/// snapshot.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SamplingAuditSnapshot {
    /// Whether the organ is observing, blind, or simply has not seen a
    /// request yet. Never collapse this to its counts when rendering.
    pub state: AuditState,
    /// Polls that saw slots but could not attribute them to one intent.
    ///
    /// Beside the state rather than inside it: [`AuditState`] answers "can
    /// this organ see", and abstaining is something an organ that *can* see
    /// does. A large count here next to zero comparisons means the traffic is
    /// too heterogeneous to attribute, which is a different problem from
    /// blindness and wants a different fix.
    pub skipped_ambiguous: u64,
    /// Client sampling fields that could not be read as sent.
    pub client_fields_rejected: u64,
    /// Client sampling fields dropped by the trust gate.
    pub client_fields_discarded: u64,
    /// Most recent field-level disagreements, oldest first.
    pub recent_divergences: Vec<Divergence>,
    /// The `/props` baseline reading for the running model. See
    /// [`crate::props`] — this is the half that catches a pin bump.
    ///
    /// Carries its own three states rather than being an `Option`, so a read
    /// that failed cannot render as one that has not happened yet.
    pub baseline: crate::props::BaselineState,
    /// What gglib's own requests do with the running model's published
    /// sampler defaults.
    pub published: PublishedOverrides,
    /// Resolved `reasoning_effort` levels stage 5b deleted before sending.
    ///
    /// The one thing on this snapshot that no wire observation could ever
    /// corroborate — see [`EffortSuppressions`].
    pub effort_suppressed: EffortSuppressions,
    /// The two reasoning controls: what the template says about them, what the
    /// last request resolved, and why none of it is an observation.
    ///
    /// Structurally different from every other field here. The rest of this
    /// snapshot reports a comparison between gglib's intent and llama-server's
    /// echo; there is no echo for these two (see the module docs), so this is
    /// gglib's own account, carried with
    /// [`WIRE_BLIND_REASON`](crate::audit_records::WIRE_BLIND_REASON) so no
    /// surface can render it as a confirmed reading.
    pub reasoning: ReasoningReadback,
    /// Which client field names were dropped, and how often each.
    ///
    /// [`Self::client_fields_discarded`] is the total; this is what it was made
    /// of. Bounded — see [`crate::audit_records`].
    pub client_field_names: ClientFieldNames,
}

/// What gglib is sending against what this model's GGUF publishes.
///
/// # A different question from the baseline check, on purpose
///
/// [`crate::props`]'s check asks *"has this build's default table moved?"* and
/// must abstain wherever attribution fails, because a wrong verdict there
/// re-opens or falsely satisfies [ADR 0003]'s deletion criterion. This asks
/// *"is gglib displacing the model author's recommendation?"* — both sides of
/// which gglib knows exactly, with no slot correlation and no sampling bias.
///
/// So the two report the same field differently and neither is wrong.
/// `/props` says `ModelSupplied` because the *build's* value is unobservable
/// there; this says `Overridden` because gglib's request body wins over the
/// table `/props` renders. A reader seeing only the first would reasonably
/// conclude the model's value is what the sampler uses.
///
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PublishedOverrides {
    /// Resolved intents folded in since this model launched.
    ///
    /// **Zero means nothing has been compared, never "nothing is overridden".**
    /// [`AuditState`]'s rule applied to this section: the fields below are
    /// empty both when a model publishes nothing and when no request has been
    /// resolved yet, and those license opposite conclusions.
    pub intents: u64,
    /// One entry per field this model publishes, in
    /// [`MODEL_SAMPLING_KEYS`](gglib_core::domain::MODEL_SAMPLING_KEYS) order.
    ///
    /// Empty on a model that publishes nothing, which is almost all of them.
    pub fields: Vec<PublishedOverrideField>,
}

/// One published field and what gglib's most recent intent did with it.
///
/// The most recent rather than an aggregate, because with
/// `trust_client_sampling` off every request against one model and profile
/// resolves identically — the same property [ADR 0004] finding 4 relies on for
/// attribution. [`PublishedOverrides::intents`] is what says whether that
/// premise held.
///
/// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PublishedOverrideField {
    /// gglib's wire name for the parameter.
    pub field: &'static str,
    /// The GGUF key carrying it, e.g. `general.sampling.penalty_repeat`.
    pub key: &'static str,
    /// What gglib is doing with it.
    #[serde(flatten)]
    pub state: PublishedOverrideState,
}

/// The verdict arm of [`PublishedOverrideField`].
///
/// Mirrors [`SamplingOverride`](gglib_core::domain::SamplingOverride) minus its
/// `NotPublished` arm — a field with nothing published is absent from
/// [`PublishedOverrides::fields`] rather than carried as an empty verdict.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PublishedOverrideState {
    /// gglib names nothing, so llama.cpp applies the model author's value.
    Deferred {
        /// What the sampler will use.
        published: f64,
    },
    /// gglib sends the same number the model published.
    Restated {
        /// The value both sides name.
        published: f64,
    },
    /// gglib sends a different number. The one arm that warrants a warning.
    Overridden {
        /// What the model author published.
        published: f64,
        /// What gglib puts on the wire instead.
        sending: f64,
    },
    /// The published value could not be read, so gglib cannot say what it
    /// displaced. Never rendered as an override — [ADR 0004] decision 3.
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    Unreadable,
}

/// Render a rung for a log line or a snapshot: a name when the value came from
/// a layer, otherwise what kind of fallback supplied it.
fn describe_source(source: ParamSource, names: &[&'static str]) -> String {
    match source {
        ParamSource::Layer(i) => (*names.get(i).unwrap_or(&"?")).to_string(),
        ParamSource::Floor => "floor".to_string(),
        ParamSource::FloorCoupled => "floor (coupled)".to_string(),
        ParamSource::Unset => "unset".to_string(),
        // Unreachable from both callers, for different reasons. `compare`
        // renders the seven readback fields and never a reasoning control
        // (finding 7a); [`SuppressedEffort`] captures the rung *before* stage
        // 5b overwrites it, and no floor names an effort. Labelled rather than
        // `unreachable!` — a mislabelled rung in an audit record is a smaller
        // fault than a panic on the request path.
        ParamSource::SuppressedByTemplate => "suppressed (template)".to_string(),
    }
}

#[cfg(test)]
#[path = "sampling_audit_tests.rs"]
mod sampling_audit_tests;
