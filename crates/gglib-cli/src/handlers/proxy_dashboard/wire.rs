//! The server's JSON contract, mirrored locally for reading only.
//!
//! `Deserialize`-only, with no `deny_unknown_fields`, so this client tolerates
//! a proxy that has grown a field it does not know about — and `#[serde(default)]`
//! throughout, so it tolerates one that has not grown a field yet. Both
//! directions matter: `gglib proxy dashboard` is routinely pointed at a proxy
//! from a different build.
//!
//! `slots` is the exception, reusing [`gglib_proxy::slots::SlotSnapshot`]
//! directly — llama.cpp's `/slots` schema has shifted shape more than once,
//! and every shift used to mean editing the same `tokens_in_use()` fallback
//! chain in two crates.

use std::collections::BTreeMap;

use gglib_proxy::slots::SlotSnapshot;
use serde::Deserialize;

use super::wire_sampling::SamplingAudit;

#[derive(Debug, Deserialize)]
pub(super) struct DashboardSnapshot {
    pub(super) active_connections: Vec<ActiveConnectionSnapshot>,
    pub(super) slots_available: bool,
    #[serde(default)]
    pub(super) slots: Vec<SlotSnapshot>,
    #[serde(default)]
    pub(super) slots_status: Option<String>,
    pub(super) total_requests: u64,
    /// Prompt-cache configuration and reuse. `None` until the first request
    /// resolves a model, and on a proxy older than this field.
    #[serde(default)]
    pub(super) cache: Option<CacheStatus>,
    /// Agent-path prompt-cache reuse (GUI chat) — a separate
    /// population from [`CacheStatus::usage`]. Top-level and always present,
    /// since it does not depend on a resolved model; `default` on a proxy older
    /// than this field.
    #[serde(default)]
    pub(super) agent_usage: CacheUsage,
    /// VRAM residency and the admission queue. `default` on a proxy older than
    /// this field, which renders as an empty resident set.
    #[serde(default)]
    pub(super) admission: AdmissionSnapshot,
    /// Per-model defect counts, keyed by the model name requests carry.
    ///
    /// Process-lifetime and reset on restart, deliberately (ADR 0006). Nothing
    /// acts on them; they are diagnosis, and this dashboard is where a person
    /// reads them. `default` on a proxy older than this field.
    #[serde(default)]
    pub(super) per_model_defects: BTreeMap<String, ModelDefectCounts>,
    /// The Tier C sampling readback. `None` on a proxy older than this field.
    ///
    /// Only the parts this dashboard renders are mirrored — the reasoning
    /// controls and the discarded client field names. The divergence list and
    /// the `/props` baseline have a surface already (the GUI panel), and
    /// mirroring them here would be an obligation to keep two renderers in
    /// step for no reader.
    #[serde(default)]
    pub(super) sampling_audit: Option<SamplingAudit>,
}

/// Mirror of `gglib_core::domain::defects::ModelDefectCounts`.
///
/// Every field defaults, so a proxy that predates any one counter renders it
/// as zero rather than failing the whole frame.
#[derive(Debug, Default, Deserialize)]
pub(super) struct ModelDefectCounts {
    #[serde(default)]
    pub(super) requests: u64,
    #[serde(default)]
    pub(super) loop_guard_trips: u64,
    #[serde(default)]
    pub(super) repairs_attempted: u64,
    #[serde(default)]
    pub(super) repairs_succeeded: u64,
    #[serde(default)]
    pub(super) stream_errors: u64,
    #[serde(default)]
    pub(super) truncated_generations: u64,
    #[serde(default)]
    pub(super) empty_responses: u64,
    #[serde(default)]
    pub(super) reasoning_only: u64,
    #[serde(default)]
    pub(super) dialect_residue: u64,
    #[serde(default)]
    pub(super) unvalidatable_schemas: u64,
    #[serde(default)]
    pub(super) normalization_errors: u64,
    #[serde(default)]
    pub(super) identical_result_repeats: u64,
    #[serde(default)]
    pub(super) repeats_not_evaluated: u64,
    #[serde(default)]
    pub(super) repeats_rescued: u64,
}

impl ModelDefectCounts {
    /// Whether this model has anything worth reading — not quite the same as
    /// whether anything went wrong.
    ///
    /// `requests` is the denominator, not a defect — a model that served a
    /// thousand clean turns has nothing to report, and saying so at length
    /// would bury the model that does.
    ///
    /// The three observational members are not faults. They are included
    /// because the dashboard is their only reader: a model whose sole signal
    /// is conversations going in circles is precisely the one to look at, as
    /// is one whose joins never succeed, and as is one whose repeats are all
    /// being rescued. Excluding any of them here would hide that model
    /// entirely. All are counted once per turn, so they stay
    /// sparse enough not to bury anything.
    pub(super) const fn is_clean(&self) -> bool {
        self.loop_guard_trips == 0
            && self.repairs_attempted == 0
            && self.stream_errors == 0
            && self.truncated_generations == 0
            && self.empty_responses == 0
            && self.dialect_residue == 0
            && self.unvalidatable_schemas == 0
            && self.normalization_errors == 0
            // Not a defect, but still something to report: a model whose only
            // signal is repeated calls returning identical results has not
            // failed at anything gglib measures, and is the exact case this
            // dashboard exists to make visible. Excluding it here would hide
            // the model from the listing entirely.
            && self.identical_result_repeats == 0
            // Listed too, and for the same reason: a fleet where the join
            // never succeeds reads as a clean one, and that is the reading
            // this counter exists to prevent anyone acting on.
            && self.repeats_not_evaluated == 0
            // And the reading ADR 0010's kill criteria rest on. A fleet whose
            // repeats are all rescued by a moving answer is one where the
            // guard has effectively stopped guarding, which is invisible from
            // the two above.
            && self.repeats_rescued == 0
    }
}

/// Mirror of `gglib_core::domain::AdmissionSnapshot`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct AdmissionSnapshot {
    #[serde(default)]
    pub(super) slots: Vec<ResidentSlotSnapshot>,
    #[serde(default)]
    pub(super) queued: Vec<QueuedModelSnapshot>,
    #[serde(default)]
    pub(super) total_swaps: u64,
    #[serde(default)]
    pub(super) secondary_slot: SecondarySlotStatus,
}

/// Mirror of `gglib_core::domain::ResidentSlotSnapshot`.
#[derive(Debug, Deserialize)]
pub(super) struct ResidentSlotSnapshot {
    pub(super) model_name: String,
    #[serde(default)]
    pub(super) inflight: u32,
    #[serde(default)]
    pub(super) is_primary: bool,
    #[serde(default)]
    pub(super) resident_for_secs: u64,
}

/// Mirror of `gglib_core::domain::QueuedModelSnapshot`.
#[derive(Debug, Deserialize)]
pub(super) struct QueuedModelSnapshot {
    pub(super) model_name: String,
    #[serde(default)]
    pub(super) waiting: usize,
    #[serde(default)]
    pub(super) oldest_wait_ms: u64,
}

/// Mirror of `gglib_core::domain::SecondarySlotStatus`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct SecondarySlotStatus {
    #[serde(default)]
    pub(super) detail: String,
}

/// Mirror of `gglib_proxy::dashboard::CacheStatus`.
#[derive(Debug, Deserialize)]
pub(super) struct CacheStatus {
    #[serde(default)]
    pub(super) disk_enabled: bool,
    #[serde(default)]
    pub(super) disk_suppressed_for_model: bool,
    #[serde(default)]
    pub(super) ram_budget_mb: Option<u64>,
    #[serde(default)]
    pub(super) ram_state: String,
    #[serde(default)]
    pub(super) warnings: Vec<String>,
    #[serde(default)]
    pub(super) usage: CacheUsage,
}

/// Mirror of `gglib_core::cache_metrics::CacheUsage`.
///
/// Raw counts only — the server publishes no derived "time saved" figure, so
/// there is none to render here either.
#[derive(Debug, Default, Deserialize)]
pub(super) struct CacheUsage {
    #[serde(default)]
    pub(super) reporting_requests: u64,
    #[serde(default)]
    pub(super) unreported_requests: u64,
    #[serde(default)]
    pub(super) prompt_tokens: u64,
    #[serde(default)]
    pub(super) cached_tokens: u64,
    #[serde(default)]
    pub(super) last_prompt_tokens: Option<u32>,
    #[serde(default)]
    pub(super) last_cached_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ActiveConnectionSnapshot {
    pub(super) model_name: String,
    pub(super) started_at_secs: u64,
    pub(super) phase: ConnectionPhase,
    #[serde(default)]
    pub(super) prompt_processed: Option<u32>,
    #[serde(default)]
    pub(super) prompt_total: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ConnectionPhase {
    Queued,
    ProcessingPrompt,
    Generating,
}

impl ConnectionPhase {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::ProcessingPrompt => "prompt",
            Self::Generating => "generating",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field the server may add later must not break deserialization —
    /// the mirror deliberately has no `deny_unknown_fields`.
    #[test]
    fn cache_status_tolerates_unknown_and_missing_fields() {
        let json = serde_json::json!({
            "disk_enabled": true,
            "ram_state": "healthy",
            "some_future_field": 42
        })
        .to_string();
        let got: CacheStatus = serde_json::from_str(&json).expect("should deserialize");
        assert!(got.disk_enabled);
        assert_eq!(got.usage.reporting_requests, 0);
        assert_eq!(got.ram_budget_mb, None);
    }

    /// A proxy older than these counters sends no `per_model_defects` at all.
    #[test]
    fn a_snapshot_without_defects_still_deserializes() {
        let json = serde_json::json!({
            "active_connections": [],
            "slots_available": false,
            "total_requests": 0
        })
        .to_string();

        let got: DashboardSnapshot = serde_json::from_str(&json).expect("should deserialize");
        assert!(got.per_model_defects.is_empty());
    }
}
