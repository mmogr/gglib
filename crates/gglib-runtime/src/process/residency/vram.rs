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

use gglib_core::domain::{SecondarySlotDecision, SlotFootprint, decide_secondary_slot};
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
pub fn footprint_of(
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
pub fn secondary_slot_decision(
    spec: &ModelLaunchSpec,
    kv_types: KvCacheTypeResolution,
    context_size: u64,
) -> SecondarySlotDecision {
    decide_secondary_slot(
        footprint_of(spec, kv_types, context_size),
        crate::system::free_gpu_memory_bytes(),
    )
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
pub const fn secondary_cache_ram() -> CacheRamSetting {
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

    fn resident(weights_bytes: u64, cache_ram_health: CacheRamHealth) -> Resident {
        Resident {
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
