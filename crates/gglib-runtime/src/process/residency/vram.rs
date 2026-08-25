//! Memory arithmetic for a two-model resident set.
//!
//! Two questions, both of which only arise because a second model may now be
//! loaded at the same time as the first:
//!
//! 1. **Does this candidate fit in VRAM alongside what is already there?**
//!    Answered by [`gglib_core::domain::decide_secondary_slot`] against a live
//!    free-VRAM reading. This module assembles the footprint and supplies the
//!    reading; the rule itself is pure domain logic and lives in core.
//! 2. **How much host RAM may its prompt cache claim?** A secondary sized as
//!    though it had the machine to itself would double-count memory the
//!    primary is already using, so the primary's own footprint is netted out
//!    first.

use gglib_core::domain::{
    BUDGET_UTILISATION, RESIDENCY_UTILISATION, SecondarySlotDecision, SlotFootprint,
    decide_secondary_slot,
};
use gglib_core::ports::ModelLaunchSpec;
use gglib_core::server_config::CacheRamSetting;

use crate::llama::args::KvCacheTypeResolution;
use crate::process::admission::Resident;

/// Estimate what `spec` would occupy in VRAM at `context_size`.
///
/// `None` when the GGUF metadata does not carry the layer/head counts needed to
/// estimate the KV cache. That is a refusal rather than an optimistic zero —
/// see [`SecondarySlotDecision::RefuseUnknownFootprint`].
#[must_use]
pub(super) fn footprint_of(
    spec: &ModelLaunchSpec,
    kv_types: KvCacheTypeResolution,
    context_size: u64,
) -> Option<SlotFootprint> {
    SlotFootprint::new(
        spec.file_size_bytes,
        spec.kv_elems_per_token,
        kv_types.k,
        kv_types.v,
        context_size,
    )
}

/// Whether `spec` may take the second resident slot right now.
///
/// The free-VRAM reading is taken at the moment of asking, not when the request
/// was queued: the primary model may have finished loading in between, and a
/// decision made against the earlier figure would be a decision about a machine
/// that no longer exists.
#[must_use]
pub(super) fn secondary_slot_decision(
    spec: &ModelLaunchSpec,
    kv_types: KvCacheTypeResolution,
    context_size: u64,
) -> SecondarySlotDecision {
    decide_secondary_slot(
        footprint_of(spec, kv_types, context_size),
        crate::system::free_gpu_memory_bytes(),
    )
}

/// What the second resident slot may claim, and therefore what a fitted
/// context must leave alone.
///
/// Mirrors gglib-core's secondary-slot ceiling, which `decide_secondary_slot`
/// enforces on any co-resident. Duplicated rather than imported because that
/// constant is crate-private to `gglib-core`; a test pins the two together by
/// asserting a candidate at this size is admitted and one byte more is not.
const SECONDARY_SLOT_RESERVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The device memory a fitted context may be sized against.
///
/// Total device capacity less a fixed reservation for the second resident
/// slot, which gglib-core hard-caps by construction.
///
/// A *fixed* reservation rather than netting out whoever is resident right
/// now, because the fitted context is part of a resident's identity: a request
/// resolving to a different one evicts and relaunches. Budgeting against the
/// live resident set makes the answer move when a co-resident loads or is
/// evicted, and each move costs the primary a full teardown, weight reload and
/// prompt re-prefill — the exact cost the second slot exists to avoid. It also
/// made the budget depend on whether some *other* model's KV shape was
/// readable, so one unreadable GGUF denied every other model a fit.
///
/// The reservation is a *top-up*, not a flat subtraction — see
/// [`fit_budget_from`]. A flat subtraction was measured across five model
/// shapes and eight devices: of the twenty-six configurations that produced a
/// fit, it changed the co-load verdict in one, and in four others cost the
/// primary between one and four rungs for a secondary that either fitted
/// anyway or could not fit regardless. Taking only what the utilisation margin
/// does not already leave keeps the co-load in the case that needed it and
/// gives the rungs back everywhere else.
///
/// It remains a per-process constant, so the budget still provably never moves.
/// The caller falls back to the undivided device when reserving would mean no
/// fit at all — see `ResidentSet::admit`.
///
/// `None` when gglib cannot read the device's memory — see
/// [`crate::system::total_device_memory_bytes`].
#[must_use]
pub(super) fn fit_budget_for() -> Option<u64> {
    Some(fit_budget_from(crate::system::total_device_memory_bytes()?))
}

/// The arithmetic, with the hardware reading lifted into a parameter so it can
/// be asserted without a GPU.
#[must_use]
fn fit_budget_from(total: u64) -> u64 {
    // Only top up what the utilisation margin does not already leave free.
    //
    // `fit_context` spends at most `BUDGET_UTILISATION` of what it is given, so
    // a primary fitted against `total - R` occupies about that fraction of it
    // and leaves roughly `(1 - BUDGET_UTILISATION) * total` free before `R` is
    // counted. On a 24 GiB card that is 2.4 GiB with `R = 0`, which already
    // covers a full-ceiling secondary; subtracting a flat 2 GiB on top
    // double-counts and costs the primary a rung for nothing.
    //
    // The target is not the ceiling itself. `decide_secondary_slot` admits a
    // candidate only when it fits *and* clears `RESIDENCY_UTILISATION` of the
    // live free reading, so admitting a full-ceiling secondary needs the
    // ceiling divided by that fraction — about 2.22 GiB, not 2.
    // Both factors come from the constants they stand for. `10 / 9` and
    // `/ 10` were hardcoded copies of `RESIDENCY_UTILISATION` and
    // `1 - BUDGET_UTILISATION`; the first is already `pub` and the second is
    // exported for this, so neither needs to be guessed.
    let target_free = scale_up(SECONDARY_SLOT_RESERVE_BYTES, RESIDENCY_UTILISATION);
    let already_free = scale(total, 1.0 - BUDGET_UTILISATION);

    // Withholding `R` yields only `BUDGET_UTILISATION * R` of extra free, since
    // the primary spends that fraction of whatever it is given — so the
    // shortfall is grossed up rather than subtracted raw.
    let withhold = scale_up(target_free.saturating_sub(already_free), BUDGET_UTILISATION);
    total.saturating_sub(withhold)
}

/// `bytes * factor`, saturating and rounding down.
#[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
fn scale(bytes: u64, factor: f64) -> u64 {
    (bytes as f64 * factor) as u64
}

/// `bytes / factor` — what you must have so that `factor` of it is `bytes`.
#[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
fn scale_up(bytes: u64, factor: f64) -> u64 {
    if factor <= 0.0 {
        return u64::MAX;
    }
    (bytes as f64 / factor) as u64
}

/// The host RAM a co-resident model's prompt cache may be sized against.
///
/// [`resolve_cache_ram`](crate::llama::args::resolve_cache_ram) budgets against
/// total system RAM on the assumption that one model is loaded. With two, that
/// assumption is wrong in the worst possible direction: both would size a cache
/// against the same free memory and together overcommit it.
///
/// Netting the existing residents' weights and KV out of the total makes the
/// second launch budget against what is actually left. Returns `0` when the
/// residents already account for everything, which
/// [`resolve_cache_ram`](crate::llama::args::resolve_cache_ram) reads as
/// "no room for a prompt cache" and reports as such.
#[must_use]
pub fn ram_available_for(total_ram_bytes: u64, existing: &[Resident]) -> u64 {
    let claimed: u64 = existing.iter().map(resident_ram_bytes).sum();
    total_ram_bytes.saturating_sub(claimed)
}

/// What one resident is assumed to be holding in host RAM.
///
/// Weights plus whatever prompt-cache budget its launch resolved. The weights
/// figure is deliberately included even on a GPU launch: llama-server memory-maps
/// the file, and on a machine tight enough for this arithmetic to matter, those
/// pages are resident.
fn resident_ram_bytes(resident: &Resident) -> u64 {
    let cache_bytes = resident
        .cache_ram_health
        .budget_mb()
        .unwrap_or(0)
        .saturating_mul(1024 * 1024);
    resident.weights_bytes.saturating_add(cache_bytes)
}

/// Whether a co-resident launch should be allowed a prompt cache at all.
///
/// A secondary model is small and its prompts are usually short and unique — an
/// embedding request has no conversation prefix to reuse. Handing it a
/// multi-gigabyte prompt cache would take memory from the primary, where reuse
/// is the difference between a fast follow-up turn and a full re-prefill, and
/// give it to a workload that cannot benefit.
#[must_use]
pub(super) const fn secondary_cache_ram() -> CacheRamSetting {
    CacheRamSetting::ExplicitMb(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::CacheRamHealth;
    use std::path::PathBuf;
    use tokio::time::Instant;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    /// The reserve is a top-up: on a device where the utilisation margin
    /// already exceeds the secondary ceiling, nothing further is withheld.
    #[test]
    fn a_roomy_device_withholds_nothing_beyond_the_utilisation_margin() {
        // 24 GiB: a tenth is 2.4 GiB, already past the 2 GiB ceiling.
        assert_eq!(fit_budget_from(24 * GIB), 24 * GIB);
    }

    /// On a device whose margin falls short, the difference is withheld — and
    /// only the difference.
    #[test]
    fn a_tight_device_withholds_only_the_shortfall() {
        // 8 GiB leaves 0.8 GiB free from the utilisation margin alone, short of
        // what admitting a full-ceiling secondary needs — so some is topped up,
        // but strictly less than the whole allowance.
        let budget = fit_budget_from(8 * GIB);
        let withheld = 8 * GIB - budget;
        assert!(
            withheld > 0,
            "a tenth of 8 GiB does not cover the secondary; something must be withheld"
        );
        assert!(
            withheld < SECONDARY_SLOT_RESERVE_BYTES,
            "withheld {withheld} — a flat subtraction would have taken the whole allowance"
        );
    }

    /// The budget is never larger than the device.
    #[test]
    fn the_budget_never_exceeds_the_device() {
        for total in [GIB, 4 * GIB, 8 * GIB, 24 * GIB, 96 * GIB, u64::MAX] {
            assert!(fit_budget_from(total) <= total);
        }
    }

    /// The property the whole reservation exists to deliver, asserted rather
    /// than re-derived: after a budget-bound fit, enough must remain for
    /// `decide_secondary_slot` to admit a full-ceiling secondary.
    ///
    /// This is what guards the two gross-ups and the margin. A bounds check on
    /// "how much was withheld" is wide enough to swallow all three — each can
    /// be reverted individually and still land inside any sane band.
    #[test]
    fn the_budget_leaves_room_for_a_full_ceiling_secondary() {
        let need = scale_up(SECONDARY_SLOT_RESERVE_BYTES, RESIDENCY_UTILISATION);
        for gib in [4_u64, 6, 8, 12, 16, 24, 32, 48] {
            let total = gib * GIB;
            let free = total - scale(fit_budget_from(total), BUDGET_UTILISATION);
            assert!(
                free >= need,
                "at {gib} GiB the fit leaves {free} free, short of the {need} \
                 a full-ceiling secondary needs"
            );
        }
    }

    /// The reserve must match the ceiling `decide_secondary_slot` enforces, or
    /// the primary either starves the second slot or leaves memory unused.
    /// The constant is duplicated because `SECONDARY_MAX_BYTES` is private to
    /// `gglib-core`; this is what keeps the copy honest.
    #[test]
    fn the_secondary_reserve_matches_the_secondary_ceiling() {
        // A candidate exactly at the reserve must be admissible, and one a
        // byte over must not — which pins our copy to theirs from the outside.
        let free = 64 * GIB;
        // Weights-only footprints, so the number under test is the total.
        let at_ceiling = SlotFootprint::new(
            SECONDARY_SLOT_RESERVE_BYTES,
            Some(gglib_core::domain::KvElemsPerToken { k: 0, v: 0 }),
            gglib_core::cache_config::KvCacheType::F16,
            gglib_core::cache_config::KvCacheType::F16,
            0,
        )
        .expect("known kv shape");
        let over = SlotFootprint::new(
            SECONDARY_SLOT_RESERVE_BYTES + 1,
            Some(gglib_core::domain::KvElemsPerToken { k: 0, v: 0 }),
            gglib_core::cache_config::KvCacheType::F16,
            gglib_core::cache_config::KvCacheType::F16,
            0,
        )
        .expect("known kv shape");
        assert!(decide_secondary_slot(Some(at_ceiling), Some(free)).is_grant());
        assert!(
            !decide_secondary_slot(Some(over), Some(free)).is_grant(),
            "a byte over our reserve must be refused by the real ceiling"
        );
    }

    fn resident(weights_bytes: u64, cache_ram_health: CacheRamHealth) -> Resident {
        Resident {
            model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
            model_id: 1,
            model_name: "m".to_string(),
            context_size: 4096,
            port: 8080,
            model_path: PathBuf::new(),
            slot_restore_supported: true,
            cache_ram_health,
            narration: None,
            inflight: 0,
            resident_since: Instant::now(),
            weights_bytes,
        }
    }

    #[test]
    fn nothing_resident_leaves_the_whole_machine_available() {
        assert_eq!(ram_available_for(64 * GIB, &[]), 64 * GIB);
    }

    /// The regression this exists to prevent: two models each sizing a prompt
    /// cache against the same free memory.
    #[test]
    fn a_resident_models_weights_and_cache_are_both_netted_out() {
        let primary = resident(9 * GIB, CacheRamHealth::Healthy { mb: 8192 });

        assert_eq!(
            ram_available_for(64 * GIB, &[primary]),
            64 * GIB - 9 * GIB - 8 * GIB,
        );
    }

    #[test]
    fn a_model_with_no_prompt_cache_only_claims_its_weights() {
        let primary = resident(9 * GIB, CacheRamHealth::DisabledByUser);
        assert_eq!(ram_available_for(64 * GIB, &[primary]), 55 * GIB);
    }

    /// Overcommitted rather than negative: a saturating floor is what
    /// `resolve_cache_ram` expects, and it reports the result honestly as
    /// "no room" rather than wrapping into an enormous budget.
    #[test]
    fn residents_larger_than_the_machine_leave_nothing_rather_than_wrapping() {
        let hog = resident(80 * GIB, CacheRamHealth::Healthy { mb: 8192 });
        assert_eq!(ram_available_for(64 * GIB, &[hog]), 0);
    }

    #[test]
    fn multiple_residents_are_all_netted_out() {
        let a = resident(9 * GIB, CacheRamHealth::Healthy { mb: 4096 });
        let b = resident(GIB, CacheRamHealth::DisabledByUser);

        assert_eq!(ram_available_for(64 * GIB, &[a, b]), 64 * GIB - 14 * GIB);
    }

    /// A co-resident model must never take prompt-cache memory from the
    /// primary, whose reuse profile is the one that actually benefits.
    #[test]
    fn a_secondary_gets_no_prompt_cache() {
        assert_eq!(secondary_cache_ram(), CacheRamSetting::ExplicitMb(0));
    }
}
