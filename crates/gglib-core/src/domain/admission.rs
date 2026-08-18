//! What the admission queue and the VRAM resident set look like right now.
//!
//! The serializable half of admission control. `gglib-runtime` owns the live
//! state and the scheduling decisions; this module owns the shape those
//! decisions are reported in, so `gglib-proxy` can put them on
//! `GET /v1/proxy/status` without depending on the runtime crate.
//!
//! Everything here is a point-in-time projection, the same way the proxy's
//! active-connection registry projects itself for the dashboard. Nothing here
//! is authoritative; reading a stale snapshot is always safe.
//!
//! ## Why the reasons are carried, not just the numbers
//!
//! A user looking at an empty second slot on a card with 12 GB free needs to be
//! told *why*. So [`SecondarySlotStatus`] carries a stable label for styling
//! and a ready-to-render sentence, in the same shape
//! [`CacheRamHealth`](crate::domain::CacheRamHealth) established for the prompt
//! cache. Consumers branch on the label and print the detail; they never parse
//! the prose.

use serde::Serialize;

use crate::domain::residency::SecondarySlotDecision;

/// One model resident in VRAM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ResidentSlotSnapshot {
    /// Slot index. `0` is the primary.
    pub slot: usize,
    /// Name of the resident model.
    pub model_name: String,
    /// Database id of the resident model.
    pub model_id: u32,
    /// Port its llama-server is listening on.
    pub port: u16,
    /// Requests currently holding a lease on this slot.
    ///
    /// A slot serving anything at all can never be evicted — a swap must not
    /// preempt a live generation.
    pub inflight: u32,
    /// Whether this is the primary slot, i.e. the one chat traffic and the
    /// llama.cpp `/slots` poller follow.
    pub is_primary: bool,
    /// Seconds this model has been resident.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub resident_for_secs: u64,
}

/// Requests waiting for one model that is not currently resident.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct QueuedModelSnapshot {
    /// The model they are waiting for.
    pub model_name: String,
    /// How many requests are queued.
    pub waiting: usize,
    /// Age of the oldest waiter, in milliseconds. A figure that keeps climbing
    /// is the sign of a model that never goes idle long enough to be swapped
    /// out — see the runtime's `admission` module for why that is bounded by a
    /// deadline rather than by preemption.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub oldest_wait_ms: u64,
}

/// Why the second VRAM slot is or is not in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct SecondarySlotStatus {
    /// Stable machine-readable label, for styling. One of `resident`,
    /// `available`, `too_large`, `no_headroom`, `unknown_footprint`,
    /// `unknown_budget`.
    ///
    /// A `&'static str`, which ts-rs would render as a bare `string` — losing
    /// the exhaustiveness the GUI's icon table and tone function are written
    /// against. The override restates the closed set above, the same way
    /// `gglib_proxy::dashboard::CacheSnapshot::ram_state` does.
    ///
    /// Three places set it, and all three must stay inside that set:
    /// [`Self::default`] and [`Self::resident`] each write one literal, and
    /// [`Self::from_decision`] takes `SecondarySlotDecision::label` for every
    /// refusal. `label` also answers `"grant"`, which never reaches here —
    /// `from_decision` maps a grant to `"available"` before consulting it.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(type = "\"resident\" | \"available\" | \"too_large\" \
                   | \"no_headroom\" | \"unknown_footprint\" | \"unknown_budget\"")
    )]
    pub state: &'static str,
    /// Ready-to-render explanation. Phrased for display rather than parsing —
    /// consumers branch on [`Self::state`].
    pub detail: String,
}

impl Default for SecondarySlotStatus {
    fn default() -> Self {
        Self {
            state: "available",
            detail: "No second model has been requested yet.".to_string(),
        }
    }
}

impl SecondarySlotStatus {
    /// The status for a slot that currently holds `model_name`.
    #[must_use]
    pub fn resident(model_name: &str) -> Self {
        Self {
            state: "resident",
            detail: format!("{model_name} is co-resident and never waits for a swap."),
        }
    }

    /// Render the most recent refusal as a status.
    ///
    /// A [`SecondarySlotDecision::Grant`] reaching here means the co-load was
    /// attempted but has not completed (or failed at spawn); it reports as
    /// available rather than resident, since nothing is loaded.
    #[must_use]
    pub fn from_decision(decision: SecondarySlotDecision) -> Self {
        let detail = match decision {
            SecondarySlotDecision::Grant { .. } => {
                "A second model fits and is being loaded.".to_string()
            }
            SecondarySlotDecision::RefuseTooLarge {
                footprint_bytes,
                ceiling_bytes,
            } => format!(
                "The requested model needs about {} — too large for the second slot, which is \
                 capped at {}. It will be swapped in instead.",
                format_bytes(footprint_bytes),
                format_bytes(ceiling_bytes),
            ),
            SecondarySlotDecision::RefuseNoHeadroom {
                footprint_bytes,
                free_bytes,
            } => format!(
                "Not enough free VRAM to keep a second model loaded: it needs about {}, and only \
                 {} is free.",
                format_bytes(footprint_bytes),
                format_bytes(free_bytes),
            ),
            SecondarySlotDecision::RefuseUnknownFootprint => {
                "The requested model's memory footprint could not be estimated from its GGUF \
                 metadata, so it is swapped in rather than co-loaded."
                    .to_string()
            }
            SecondarySlotDecision::RefuseUnknownBudget => {
                "gglib cannot read this machine's free VRAM, so it keeps one model loaded at a \
                 time. Free-VRAM readings are available on NVIDIA and Apple Silicon."
                    .to_string()
            }
        };

        // A grant that has not finished loading holds nothing, so it reports as
        // available rather than borrowing the decision's own label. Every
        // refusal keeps its label verbatim, so styling can branch on it.
        let state = if decision.is_grant() {
            "available"
        } else {
            decision.label()
        };

        Self { state, detail }
    }
}

/// Everything the admission queue and resident set look like right now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct AdmissionSnapshot {
    /// Models resident in VRAM, primary first. Empty before anything has been
    /// launched.
    pub slots: Vec<ResidentSlotSnapshot>,
    /// Models with requests waiting, oldest waiter first. Empty in the steady
    /// state — a non-empty list means traffic is being batched behind a swap.
    pub queued: Vec<QueuedModelSnapshot>,
    /// Requests that have waited in the queue since the runtime started.
    /// Compare against [`Self::total_swaps`]: a large ratio is the queue doing
    /// its job, batching many requests behind one swap.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_queued: u64,
    /// Model swaps performed since the runtime started.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_swaps: u64,
    /// Why the second slot is or is not in use.
    pub secondary_slot: SecondarySlotStatus,
}

impl AdmissionSnapshot {
    /// Total requests waiting across every model.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.queued.iter().map(|q| q.waiting).sum()
    }

    /// Total requests currently being served across every resident slot.
    #[must_use]
    pub fn inflight(&self) -> u32 {
        self.slots.iter().map(|s| s.inflight).sum()
    }
}

/// Render a byte count the way the launch banner does — one decimal, GiB above
/// a gibibyte and MiB below, so "307 MiB" does not print as "0.3 GiB".
fn format_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    #[allow(clippy::cast_precision_loss)]
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else {
        format!("{:.0} MiB", value / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn slot(name: &str, inflight: u32, is_primary: bool) -> ResidentSlotSnapshot {
        ResidentSlotSnapshot {
            slot: usize::from(!is_primary),
            model_name: name.to_string(),
            model_id: 1,
            port: 8080,
            inflight,
            is_primary,
            resident_for_secs: 10,
        }
    }

    #[test]
    fn an_empty_snapshot_reports_nothing_waiting_and_nothing_running() {
        let snapshot = AdmissionSnapshot::default();
        assert_eq!(snapshot.waiting(), 0);
        assert_eq!(snapshot.inflight(), 0);
        assert_eq!(snapshot.secondary_slot.state, "available");
    }

    #[test]
    fn totals_aggregate_across_slots_and_queues() {
        let snapshot = AdmissionSnapshot {
            slots: vec![slot("qwen-coder", 2, true), slot("nomic-embed", 1, false)],
            queued: vec![QueuedModelSnapshot {
                model_name: "llama-3".to_string(),
                waiting: 4,
                oldest_wait_ms: 900,
            }],
            total_queued: 12,
            total_swaps: 2,
            secondary_slot: SecondarySlotStatus::resident("nomic-embed"),
        };

        assert_eq!(snapshot.inflight(), 3);
        assert_eq!(snapshot.waiting(), 4);
    }

    /// The dashboard has to explain an idle second slot on a card that plainly
    /// has room, so every refusal names its own reason.
    #[test]
    fn each_refusal_carries_a_distinct_label_and_a_populated_detail() {
        let cases = [
            SecondarySlotDecision::RefuseTooLarge {
                footprint_bytes: 10 * GIB,
                ceiling_bytes: 2 * GIB,
            },
            SecondarySlotDecision::RefuseNoHeadroom {
                footprint_bytes: 307 * MIB,
                free_bytes: 200 * MIB,
            },
            SecondarySlotDecision::RefuseUnknownFootprint,
            SecondarySlotDecision::RefuseUnknownBudget,
        ];

        let mut seen = Vec::new();
        for case in cases {
            let status = SecondarySlotStatus::from_decision(case);
            assert!(!status.detail.is_empty(), "{case:?} produced no detail");
            assert!(!seen.contains(&status.state), "duplicate label {case:?}");
            seen.push(status.state);
        }
    }

    /// The figures a refusal quotes must appear in the sentence it renders —
    /// "not enough VRAM" without a number is not actionable.
    #[test]
    fn a_headroom_refusal_names_both_figures() {
        let status = SecondarySlotStatus::from_decision(SecondarySlotDecision::RefuseNoHeadroom {
            footprint_bytes: 307 * MIB,
            free_bytes: 200 * MIB,
        });

        assert!(status.detail.contains("307 MiB"), "{}", status.detail);
        assert!(status.detail.contains("200 MiB"), "{}", status.detail);
    }

    #[test]
    fn resident_status_names_the_model() {
        let status = SecondarySlotStatus::resident("nomic-embed-text");
        assert_eq!(status.state, "resident");
        assert!(status.detail.contains("nomic-embed-text"));
    }

    #[test]
    fn snapshot_always_serializes() {
        let json = serde_json::to_string(&AdmissionSnapshot::default())
            .expect("AdmissionSnapshot must always serialize");
        assert!(json.contains("secondary_slot"));
        assert!(json.contains("total_swaps"));
    }

    // ── format_bytes ─────────────────────────────────────────────────────

    #[test]
    fn bytes_render_as_gib_above_a_gibibyte_and_mib_below() {
        assert_eq!(format_bytes(307 * MIB), "307 MiB");
        assert_eq!(format_bytes(2 * GIB), "2.0 GiB");
        assert_eq!(format_bytes(GIB + 512 * MIB), "1.5 GiB");
    }
}
