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
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use gglib_core::domain::{InferenceConfig, ModelSamplingDefaults, ParamSource, SamplingOverride};
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
    /// Storage only, deliberately: not part of [`Self::snapshot`], because
    /// nothing consumes the observation yet — the dashboards and the effort
    /// gate arrive in later PRs of the reasoning-controls arc.
    template_caps: Mutex<gglib_core::domain::TemplateCapsState>,
    /// The running model's name and what its GGUF publishes, set once per
    /// launch by the poller. `None` until the first poll names a model; the
    /// inner `Option` is `None` for a model with no metadata read.
    model_sampling: Mutex<Option<(String, Option<ModelSamplingDefaults>)>>,
    /// The published-vs-sent comparison, refreshed from each resolved intent.
    published: Mutex<PublishedOverrides>,
}

impl SamplingAuditStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what the request pipeline resolved, for the counters that do not
    /// need a wire observation to be meaningful.
    pub fn record_intent(&self, decision: &SamplingDecision) {
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
        }
    }
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
            dynatemp_range: source,
            dynatemp_exponent: source,
            top_n_sigma: source,
            dry_multiplier: source,
            dry_base: source,
            dry_allowed_length: source,
            dry_penalty_last_n: source,
            frequency_penalty: source,
            reasoning_effort: source,
            reasoning_budget_tokens: source,
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

    // ── Abstention ────────────────────────────────────────────────────────

    fn intent_at(temp: f32) -> SamplingDecision {
        decision(
            InferenceConfig {
                temperature: Some(temp),
                ..Default::default()
            },
            all_from(ParamSource::Layer(3)),
        )
    }

    /// The measured common case: four concurrent turns resolving identically,
    /// which is what the default configuration produces because every
    /// compared field comes from the ladder rather than the client. Measured
    /// 0 ambiguous polls out of 10 against a real server.
    #[test]
    fn identical_intents_are_compared_not_skipped() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let intents = vec![intent_at(0.12), intent_at(0.12), intent_at(0.12)];
        let slots = vec![observed.clone(), observed.clone(), observed];

        let out = compare_poll(&intents, &slots);
        assert_eq!(out.comparisons, 3);
        assert_eq!(out.divergences, 0);
        assert_eq!(out.skipped_ambiguous, 0);
    }

    /// gglib cannot join a slot to the request that filled it, so when the
    /// intents in flight disagree an observation cannot be attributed to one.
    /// Guessing would produce a divergence that is an artefact of the guess.
    #[test]
    fn disagreeing_intents_abstain_rather_than_guess() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let intents = vec![intent_at(0.12), intent_at(0.90)];
        let slots = vec![observed.clone(), observed];

        let out = compare_poll(&intents, &slots);
        assert_eq!(out.comparisons, 0, "nothing may be compared");
        assert_eq!(out.divergences, 0);
        assert_eq!(out.skipped_ambiguous, 2, "and the gap is counted");
    }

    /// `max_tokens` is client-authoritative and varies request to request,
    /// while the compared fields do not. Keying ambiguity on the whole
    /// decision would abstain on essentially every poll.
    #[test]
    fn a_differing_max_tokens_alone_is_not_ambiguity() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let mut a = intent_at(0.12);
        let mut b = intent_at(0.12);
        a.resolved.max_tokens = Some(128);
        b.resolved.max_tokens = Some(4096);

        let out = compare_poll(&[a, b], std::slice::from_ref(&observed));
        assert_eq!(out.comparisons, 1);
        assert_eq!(out.skipped_ambiguous, 0);
    }

    /// **A false positive this used to produce.** llama-server is reachable
    /// directly, and `llama::args::sampling` records that as the one
    /// population the deleted launch flags ever served. Such a request
    /// occupies a slot gglib has no intent for, and comparing it against
    /// gglib's own intent invented a divergence with a confident provenance
    /// string attached — on the instrument whose only value is being worth
    /// believing when it fires.
    #[test]
    fn a_busy_slot_gglib_cannot_account_for_abstains_rather_than_diverging() {
        let mine: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let someone_elses = SlotParams {
            temperature: Some(1.9),
            ..mine.clone()
        };
        // One gglib request in flight, two slots busy: the second is not ours.
        let out = compare_poll(&[intent_at(0.12)], &[mine, someone_elses]);

        assert_eq!(out.divergences, 0, "must not report a stranger's slot");
        assert_eq!(out.comparisons, 0);
        assert_eq!(out.skipped_ambiguous, 2, "the whole poll is unattributable");
    }

    /// The surplus is not compared "as far as it goes". Slots arrive in no
    /// particular order, so comparing `min(observed, intents)` of them would
    /// pick an arbitrary subset — making the false positive rarer without
    /// making it less wrong.
    #[test]
    fn a_surplus_slot_abstains_over_the_whole_poll_not_just_the_extra() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let out = compare_poll(
            &[intent_at(0.12), intent_at(0.12)],
            &[observed.clone(), observed.clone(), observed],
        );

        assert_eq!(out.comparisons, 0, "not two of the three");
        assert_eq!(out.skipped_ambiguous, 3);
    }

    /// Fewer busy slots than intents is the ordinary case — a request can be
    /// queued, or between shaping and reaching a slot — and must still compare.
    #[test]
    fn fewer_busy_slots_than_intents_still_compares() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let intents = vec![intent_at(0.12), intent_at(0.12), intent_at(0.12)];

        let out = compare_poll(&intents, std::slice::from_ref(&observed));
        assert_eq!(out.comparisons, 1);
        assert_eq!(out.skipped_ambiguous, 0);
    }

    #[test]
    fn no_recorded_intent_compares_nothing() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        let out = compare_poll(&[], std::slice::from_ref(&observed));
        assert_eq!(out.comparisons, 0);
        assert_eq!(
            out.skipped_ambiguous, 0,
            "absence of intent is not ambiguity"
        );
    }

    #[test]
    fn a_real_divergence_is_counted_once_per_slot() {
        let observed: SlotParams = serde_json::from_str(REAL_SLOT).unwrap();
        // One intent per busy slot: two turns in flight, both resolving to
        // 0.90 while the slots report 0.12. Fewer intents than slots is the
        // unattributable case below, not this one.
        let intents = vec![intent_at(0.90), intent_at(0.90)];
        let slots = vec![observed.clone(), observed];

        let out = compare_poll(&intents, &slots);
        assert_eq!(out.comparisons, 2);
        assert_eq!(out.divergences, 2);
        assert_eq!(out.found.len(), 2);
        assert_eq!(out.found[0].field, "temperature");
    }

    // ── Store ─────────────────────────────────────────────────────────────

    fn poll(comparisons: u64, divergences: u64, skipped: u64) -> PollOutcome {
        PollOutcome {
            comparisons,
            divergences,
            skipped_ambiguous: skipped,
            found: Vec::new(),
        }
    }

    #[test]
    fn a_fresh_store_has_not_yet_observed() {
        let store = SamplingAuditStore::new();
        assert_eq!(store.state(), AuditState::NotYetObserved);
        assert!(!store.state().is_observing());
    }

    /// The trap this store exists to avoid: a poll that compared nothing is
    /// not evidence of recovery, so it must not clear the latch.
    #[test]
    fn a_poll_that_compared_nothing_leaves_the_store_blind() {
        let store = SamplingAuditStore::new();
        store.mark_blind("upstream gone");

        store.record_poll(&poll(0, 0, 3));

        assert!(matches!(store.state(), AuditState::Blind { .. }));
        assert_eq!(store.snapshot().skipped_ambiguous, 3);
    }

    #[test]
    fn a_poll_that_compared_something_clears_the_latch() {
        let store = SamplingAuditStore::new();
        store.mark_blind("upstream gone");

        store.record_poll(&poll(2, 1, 0));

        assert_eq!(
            store.state(),
            AuditState::Comparing {
                comparisons: 2,
                divergences: 1
            }
        );
    }

    /// Abstention lives beside the state, not inside it: an organ that can
    /// see but cannot attribute is a different problem from a blind one, and
    /// collapsing them would hide which fix is needed.
    #[test]
    fn abstention_is_reported_without_claiming_blindness() {
        let store = SamplingAuditStore::new();
        store.record_poll(&poll(0, 0, 12));

        assert_eq!(
            store.state(),
            AuditState::NotYetObserved,
            "abstaining is something a sighted organ does"
        );
        assert_eq!(store.snapshot().skipped_ambiguous, 12);
    }

    #[test]
    fn client_field_counters_accumulate_across_requests() {
        let store = SamplingAuditStore::new();
        let mut d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));
        d.client_fields_discarded = vec!["temperature".into(), "top_p".into()];
        d.client_fields_rejected = vec![gglib_core::domain::FieldIssue::Rejected {
            field: "top_k",
            value: "banana".into(),
            expected: "an integer",
        }];

        store.record_intent(&d);
        store.record_intent(&d);

        let snap = store.snapshot();
        assert_eq!(snap.client_fields_discarded, 4);
        assert_eq!(snap.client_fields_rejected, 2);
    }

    // =========================================================================
    // seed
    // =========================================================================

    /// **The premise every reproducibility claim rests on.** A seed that was
    /// resolved but never reached the sampler would leave a benchmark reading
    /// the resulting variance as signal, with nothing on any surface to say
    /// otherwise.
    #[test]
    fn a_seed_that_did_not_reach_the_sampler_diverges() {
        let resolved = InferenceConfig {
            seed: Some(100),
            ..InferenceConfig::default()
        };
        let observed = SlotParams {
            // What llama-server reports when it drew its own seed.
            seed: Some(4_294_967_295.0),
            ..SlotParams::default()
        };

        let found = compare(&decision(resolved, all_from(ParamSource::Unset)), &observed);

        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].field, "seed");
        assert!((found[0].sent - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_seed_that_arrived_intact_does_not_diverge() {
        let resolved = InferenceConfig {
            seed: Some(100),
            ..InferenceConfig::default()
        };
        let observed = SlotParams {
            seed: Some(100.0),
            ..SlotParams::default()
        };

        assert!(compare(&decision(resolved, all_from(ParamSource::Unset)), &observed).is_empty());
    }

    /// An unseeded request has no intent to diverge from, so llama-server
    /// drawing its own random seed is the expected observation rather than a
    /// fault. Without this the readback would fire on every ordinary request.
    #[test]
    fn an_unseeded_request_does_not_diverge_on_the_servers_random_seed() {
        let observed = SlotParams {
            seed: Some(4_294_967_295.0),
            ..SlotParams::default()
        };

        assert!(
            compare(
                &decision(InferenceConfig::default(), all_from(ParamSource::Unset)),
                &observed
            )
            .is_empty()
        );
    }

    // =========================================================================
    // Published-vs-sent
    // =========================================================================

    fn publishing(pairs: &[(&str, &str)]) -> ModelSamplingDefaults {
        let metadata: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        ModelSamplingDefaults::from_metadata(&metadata)
    }

    /// **The rule this section exists to obey.** An empty field list means
    /// either "this model publishes nothing" or "nothing has been resolved
    /// yet", and those license opposite conclusions — so the intent count has
    /// to be readable separately, exactly as `AuditState` is.
    #[test]
    fn an_intent_before_any_model_is_known_compares_nothing() {
        let store = SamplingAuditStore::new();

        store.record_intent(&decision(
            InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            },
            all_from(ParamSource::Layer(3)),
        ));

        let published = store.snapshot().published;
        assert_eq!(published.intents, 0, "nothing was compared");
        assert!(published.fields.is_empty());
    }

    /// The headline case: the model asks for 0.33 and gglib resolves 1.0.
    #[test]
    fn a_resolved_value_displacing_a_published_one_is_reported_as_an_override() {
        let store = SamplingAuditStore::new();
        store.set_model_sampling(
            "qwen",
            Some(publishing(&[("general.sampling.temp", "0.33")])),
        );

        store.record_intent(&decision(
            InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            },
            all_from(ParamSource::Layer(3)),
        ));

        let published = store.snapshot().published;
        assert_eq!(published.intents, 1);
        assert_eq!(published.fields.len(), 1);
        assert_eq!(published.fields[0].field, "temperature");
        assert_eq!(published.fields[0].key, "general.sampling.temp");
        match published.fields[0].state {
            PublishedOverrideState::Overridden { published, sending } => {
                assert!((published - 0.33).abs() < 1e-9, "{published}");
                assert!((sending - 1.0).abs() < 1e-6, "{sending}");
            }
            ref other => panic!("expected overridden, got {other:?}"),
        }
    }

    /// gglib naming nothing is what lets the model's value through, and must
    /// never read as an override.
    #[test]
    fn a_value_gglib_never_names_defers_to_the_model() {
        let store = SamplingAuditStore::new();
        store.set_model_sampling(
            "qwen",
            Some(publishing(&[("general.sampling.top_p", "0.71")])),
        );

        store.record_intent(&decision(
            InferenceConfig::default(),
            all_from(ParamSource::Unset),
        ));

        let published = store.snapshot().published;
        assert_eq!(
            published.fields[0].state,
            PublishedOverrideState::Deferred { published: 0.71 }
        );
    }

    /// A model swap must not leave the previous model's comparison on display,
    /// nor carry its intent count across.
    #[test]
    fn a_model_swap_resets_the_comparison() {
        let store = SamplingAuditStore::new();
        store.set_model_sampling(
            "qwen",
            Some(publishing(&[("general.sampling.temp", "0.33")])),
        );
        store.record_intent(&decision(
            InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            },
            all_from(ParamSource::Layer(3)),
        ));
        assert_eq!(store.snapshot().published.intents, 1, "guards the premise");

        store.set_model_sampling("llama", None);

        let published = store.snapshot().published;
        assert_eq!(published.intents, 0);
        assert!(published.fields.is_empty());
    }

    /// **Two models that publish nothing compare equal.** A value-keyed reset
    /// would carry the first model's intent count into the second and report a
    /// comparison that never happened for it.
    #[test]
    fn a_swap_between_two_silent_models_still_resets() {
        let store = SamplingAuditStore::new();
        store.set_model_sampling("qwen", Some(publishing(&[])));
        store.record_intent(&decision(
            InferenceConfig::default(),
            all_from(ParamSource::Unset),
        ));
        assert_eq!(store.snapshot().published.intents, 1, "guards the premise");

        store.set_model_sampling("llama", Some(publishing(&[])));

        assert_eq!(store.snapshot().published.intents, 0);
    }

    /// The poller retries until `/props` reads, so this is called repeatedly
    /// within one launch and must not reset the count each time.
    #[test]
    fn re_setting_the_same_model_is_idempotent() {
        let store = SamplingAuditStore::new();
        let model = publishing(&[("general.sampling.temp", "0.33")]);
        store.set_model_sampling("qwen", Some(model));
        store.record_intent(&decision(
            InferenceConfig::default(),
            all_from(ParamSource::Unset),
        ));

        store.set_model_sampling("qwen", Some(model));

        assert_eq!(store.snapshot().published.intents, 1);
    }

    #[test]
    fn recent_divergences_are_bounded_and_keep_the_newest() {
        let store = SamplingAuditStore::new();
        for i in 0..MAX_RECENT_DIVERGENCES + 5 {
            store.record_poll(&PollOutcome {
                comparisons: 1,
                divergences: 1,
                skipped_ambiguous: 0,
                found: vec![Divergence {
                    field: "temperature",
                    sent: f64::from(u32::try_from(i).unwrap()),
                    observed: 0.0,
                    provenance: "floor".into(),
                }],
            });
        }

        let snap = store.snapshot();
        assert_eq!(snap.recent_divergences.len(), MAX_RECENT_DIVERGENCES);
        assert!(
            (snap.recent_divergences.last().unwrap().sent - 24.0).abs() < f64::EPSILON,
            "the newest divergence must survive eviction"
        );
    }

    #[test]
    fn the_ladder_width_matches_the_pipeline() {
        let d = decision(InferenceConfig::default(), all_from(ParamSource::Unset));
        assert_eq!(d.layer_names.len(), LADDER_RUNGS);
    }

    // ── Template-caps storage (ADR 0007) ──────────────────────────────────

    use gglib_core::domain::{TemplateCaps, TemplateCapsState};

    /// The store holds the caps tri-state beside the baseline, with the same
    /// overwrite-on-set discipline: the latest reading wins, whatever it is.
    #[test]
    fn template_caps_default_to_not_yet_read_and_hold_the_latest_reading() {
        let store = SamplingAuditStore::new();
        assert_eq!(store.template_caps(), TemplateCapsState::NotYetRead);

        let caps = TemplateCaps {
            supports_reasoning_effort: Some(true),
            ..TemplateCaps::default()
        };
        store.set_template_caps(TemplateCapsState::Read { caps: caps.clone() });
        assert_eq!(store.template_caps().caps(), Some(&caps));

        // A model swap whose read fails must replace the stale report with
        // an honest failure, not leave the previous template's caps standing.
        store.set_template_caps(TemplateCapsState::Unreadable {
            reason: "connection refused".into(),
        });
        assert_eq!(store.template_caps().caps(), None);
    }

    /// Storage only, this PR: the observation must not leak into the
    /// dashboard snapshot until the surface PR deliberately adds it.
    #[test]
    fn the_snapshot_does_not_carry_template_caps_yet() {
        let store = SamplingAuditStore::new();
        store.set_template_caps(TemplateCapsState::Read {
            caps: TemplateCaps::default(),
        });

        let json = serde_json::to_value(store.snapshot()).expect("snapshot serializes");
        assert!(
            json.get("template_caps").is_none(),
            "template caps surfaced in the snapshot before their PR: {json}"
        );
    }
}
