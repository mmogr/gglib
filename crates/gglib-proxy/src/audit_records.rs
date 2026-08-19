//! The two records the sampling readback keeps *because* the wire will never
//! confirm them.
//!
//! **Tier C — Observation** ([ADR 0001]), and a deliberate exception to how the
//! rest of [`crate::sampling_audit`] works. Everything else in that module is a
//! comparison: gglib's intent against llama-server's echo. These two are not,
//! and cannot be, because in both cases there is no echo to compare against.
//!
//! # 1. The reasoning controls, which nothing echoes
//!
//! `reasoning_effort` is spent producing a prompt before any sampler exists,
//! and `reasoning_budget_tokens` is parsed into `params.sampling` and then
//! serialised by nothing. Measured, not assumed — [ADR 0007] finding 7a, and
//! [`crate::sampling_audit`]'s module docs carry the citations and the
//! enforcement.
//!
//! So the ladder's own [`FieldSources`] entry is the *entire* account of what
//! gglib decided, and [`ReasoningReadback`] is that account made visible. It is
//! the opposite posture from the rest of the audit — a claim gglib makes about
//! itself, carrying [`WIRE_BLIND_REASON`] so that no surface can render it as a
//! confirmed observation.
//!
//! # 2. The discarded client fields, which never left the proxy
//!
//! The trust gate bins a client's sampling before the body is sent, so those
//! fields are absent from the wire by construction. `client_fields_discarded`
//! aggregated them to a count, and a count cannot answer the question the
//! record exists for — *"why did my `reasoning_effort` do nothing?"*. The name
//! can. [`ClientFieldNameTally`] keeps the names beside the counts.
//!
//! **Bounded on purpose.** The names are gglib's own today — the keys of
//! [`InferenceConfig::to_openai_json_patch`] plus `UNMODELLED_SAMPLER_KEYS`, a
//! fixed set of under thirty — but this is the request path, and an unbounded
//! map keyed by anything a client can influence is a memory risk one refactor
//! away. So the table stops at [`MAX_TRACKED_FIELD_NAMES`] distinct names and
//! counts the rest in [`ClientFieldNames::untracked`] rather than silently
//! dropping them: a bound nobody can see is indistinguishable from a bound
//! nobody hit. **Names only, never values** — a client's `temperature` is its
//! business, and a rejected value is already rendered into the `debug!` line by
//! [`FieldIssue`]'s `Display`.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
//! [`FieldSources`]: gglib_core::domain::FieldSources
//! [`InferenceConfig::to_openai_json_patch`]: gglib_core::domain::InferenceConfig::to_openai_json_patch

use std::sync::Mutex;

use gglib_core::domain::{
    FieldIssue, ReasoningEffort, Support, TemplateCapsState, reasoning_effort_support,
};
use serde::Serialize;

/// Why nothing below is corroborated, in one sentence a surface can print.
///
/// Carried on every [`ReasoningReadback`] rather than written into the CLI, the
/// GUI and the HTTP contract separately: three copies of a measured fact drift,
/// and the one that drifts will be the one somebody reads.
pub(crate) const WIRE_BLIND_REASON: &str = "llama-server echoes neither reasoning control: reasoning_effort is a chat-template kwarg \
     consumed at render time, and task_params::to_json serialises no reasoning_budget_* field \
     (ADR 0007 finding 7a). These are gglib's own record of what it sent — no readback can \
     confirm them.";

/// How many distinct client field names the discard tally will hold.
///
/// Comfortably above the fixed set gglib can actually produce (roughly
/// twenty-seven: the ladder's own wire keys plus `UNMODELLED_SAMPLER_KEYS`), so
/// the bound is a guard rather than a working limit. See the module docs for
/// why it exists at all.
pub(crate) const MAX_TRACKED_FIELD_NAMES: usize = 32;

// =============================================================================
// Template support for reasoning_effort
// =============================================================================

/// Whether the running model's template reads `reasoning_effort`, as three
/// states that are never collapsed into two.
///
/// [`TemplateCapsState`]'s discipline carried one level up, and the reason it
/// is a type rather than an `Option<bool>`: "the template does not read it" and
/// "nobody has looked yet" license opposite actions. The first is what the
/// suppression gate acts on; the second must never be, and a surface that
/// renders them the same way has re-introduced exactly the collapse ADR 0007
/// decision 3 forbids.
///
/// Three distinct causes fold into [`Self::NotYetObserved`] — no read yet, a
/// read that failed, and a caps object that did not carry the field — so each
/// carries its own reason rather than sharing a label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EffortSupportState {
    /// The observed template positively reads the variable.
    Supported,
    /// The observed template positively does not read it. The one arm the
    /// effort gate is allowed to act on.
    NotSupported,
    /// Nothing has been observed that answers the question.
    NotYetObserved {
        /// Which of the three causes applies, in words a dashboard can show.
        reason: String,
    },
}

impl EffortSupportState {
    /// Read the answer out of whatever caps state the poller last stored.
    #[must_use]
    pub fn of(caps: &TemplateCapsState) -> Self {
        match caps {
            TemplateCapsState::NotYetRead => Self::NotYetObserved {
                reason: "no /props read has completed for the running model yet".to_string(),
            },
            TemplateCapsState::Unreadable { reason } => Self::NotYetObserved {
                reason: format!("this server's template capabilities could not be read: {reason}"),
            },
            // Cloned rather than reaching into `caps.supports_reasoning_effort`
            // directly: the `Some(false)` → `No`, absent → `Unknown` mapping is
            // the domain's rule, and a second copy of it here is a second place
            // for "absent" to start meaning "no".
            TemplateCapsState::Read { caps } => match reasoning_effort_support(&Some(caps.clone()))
            {
                Support::Yes => Self::Supported,
                Support::No => Self::NotSupported,
                Support::Unknown => Self::NotYetObserved {
                    reason: "this server reported template capabilities without \
                             supports_reasoning_effort; five of the nine default to true \
                             upstream, so its absence licenses no conclusion"
                        .to_string(),
                },
            },
        }
    }
}

// =============================================================================
// The resolved reasoning controls
// =============================================================================

/// The `reasoning_effort` the most recent request resolved, and its rung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct EffortRung {
    /// The level the ladder resolved.
    pub level: ReasoningEffort,
    /// The rung that supplied it — `profile`, `model`, `global`, `cli`.
    pub source: String,
    /// Whether stage 5b then deleted it because the observed template never
    /// reads the variable. `true` means llama-server never saw this level, and
    /// a surface that renders the level without the marker is reporting a
    /// control that did nothing as though it had worked.
    pub suppressed: bool,
}

/// The `reasoning_budget_tokens` the most recent request resolved, and its rung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct BudgetRung {
    /// The cap, in tokens. `0` is the documented "stop thinking".
    pub tokens: i32,
    /// The rung that supplied it.
    pub source: String,
}

/// What one request resolved for the two reasoning controls.
///
/// A record with both halves `None` is meaningful and is *not* the same as no
/// record: it says a request was resolved and named neither control, where an
/// absent record says no request has been resolved at all. The distinction is
/// [`AuditState`](crate::sampling_audit::AuditState)'s, applied to a field that
/// has no wire observation to fall back on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ResolvedReasoning {
    /// The resolved level, suppressed or not.
    pub effort: Option<EffortRung>,
    /// The resolved budget.
    pub budget: Option<BudgetRung>,
}

/// Everything a surface needs to render the reasoning controls honestly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ReasoningReadback {
    /// Whether the running model's template reads `reasoning_effort`.
    pub effort_support: EffortSupportState,
    /// What the most recent resolved request named. `None` until one has been.
    pub latest: Option<ResolvedReasoning>,
    /// [`WIRE_BLIND_REASON`], so every surface says the same thing.
    pub wire_blind_reason: &'static str,
}

// =============================================================================
// The discarded client field names
// =============================================================================

/// One client field name and how often each kind of drop happened to it.
///
/// Both counters on one row rather than two lists: "gglib is ignoring my
/// `temperature`" and "gglib could not read it" are one lookup, and two lists
/// make a reader check two places to learn it is in neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ClientFieldTally {
    /// The wire key, as gglib names it. Never a client-supplied string — see
    /// the module docs.
    pub field: String,
    /// Times the trust gate binned it. Large by default and not a fault:
    /// `trust_client_sampling` is off, so every client-supplied sampler value
    /// is discarded by design.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub discarded: u64,
    /// Times it could not be read as sent.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub rejected: u64,
}

impl ClientFieldTally {
    /// Total drops of either kind, for ordering.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.discarded.saturating_add(self.rejected)
    }
}

/// The whole tally, ready to surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ClientFieldNames {
    /// Tracked names, most-dropped first, then alphabetical.
    pub fields: Vec<ClientFieldTally>,
    /// Drops whose name was **not** tracked because the table was already at
    /// [`MAX_TRACKED_FIELD_NAMES`]. Zero on every configuration gglib can
    /// currently produce, and reported anyway: a silent bound and a bound
    /// nobody hit look identical, and the first makes [`Self::fields`] a claim
    /// it cannot support.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub untracked: u64,
}

/// The bounded, mutex-guarded store behind [`ClientFieldNames`].
///
/// `std::sync::Mutex` following [`crate::metrics::ContextMetricsStore`]'s
/// convention: the critical section is a lookup and an increment over a vector
/// of at most [`MAX_TRACKED_FIELD_NAMES`] entries, with no `.await` inside.
/// A linear scan rather than a map, deliberately — at this size it is faster
/// than hashing, and it keeps the insertion bound trivial to read.
#[derive(Debug, Default)]
pub(crate) struct ClientFieldNameTally {
    inner: Mutex<ClientFieldNames>,
}

impl ClientFieldNameTally {
    /// Fold one request's drops into the tally.
    pub(crate) fn record(&self, discarded: &[String], rejected: &[FieldIssue]) {
        if discarded.is_empty() && rejected.is_empty() {
            return;
        }
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for field in discarded {
            bump(&mut guard, field, DropKind::Discarded);
        }
        for issue in rejected {
            bump(&mut guard, issue.field(), DropKind::Rejected);
        }
    }

    /// The tally, ordered for display.
    #[must_use]
    pub(crate) fn snapshot(&self) -> ClientFieldNames {
        let mut out = self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone();
        out.fields.sort_by(|a, b| {
            b.total()
                .cmp(&a.total())
                .then_with(|| a.field.cmp(&b.field))
        });
        out
    }
}

/// Which counter a drop belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DropKind {
    Discarded,
    Rejected,
}

/// Increment one name, or count it as untracked if the table is full.
fn bump(state: &mut ClientFieldNames, field: &str, kind: DropKind) {
    if let Some(existing) = state.fields.iter_mut().find(|t| t.field == field) {
        match kind {
            DropKind::Discarded => existing.discarded = existing.discarded.saturating_add(1),
            DropKind::Rejected => existing.rejected = existing.rejected.saturating_add(1),
        }
        return;
    }
    if state.fields.len() >= MAX_TRACKED_FIELD_NAMES {
        state.untracked = state.untracked.saturating_add(1);
        return;
    }
    state.fields.push(ClientFieldTally {
        field: field.to_owned(),
        discarded: u64::from(kind == DropKind::Discarded),
        rejected: u64::from(kind == DropKind::Rejected),
    });
}

#[cfg(test)]
#[path = "audit_records_tests.rs"]
mod audit_records_tests;
