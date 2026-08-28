//! What a *fitted* context does to the second resident slot.
//!
//! `SECONDARY_MAX_BYTES` is a hard 2 GiB ceiling on a co-resident's whole
//! footprint, weights plus KV. Before ADR 0009 every model was launched at the
//! 4096 floor, so an embedder's KV cost was a rounding error against its
//! weights and the ceiling was never the binding constraint. A fitted context
//! changes that: KV scales linearly with it, and the fit is computed against
//! the *whole device* before the queue has decided which slot the model gets.
//!
//! These tests are arithmetic, not policy — they pin what the numbers do, so
//! the trade-off is argued from a measurement rather than from a guess.

use super::*;

/// Qwen3-Embedding-0.6B at `Q8_0`, the model this slot exists to keep resident:
/// ~28 layers x 1024 hidden x 2 (K and V) is about 57k elements per token, and
/// at `Q8_0` that is roughly a byte each.
const EMBEDDER_WEIGHTS: u64 = 640 * 1024 * 1024;
const EMBEDDER_KV: KvElemsPerToken = KvElemsPerToken {
    k: 28_672,
    v: 28_672,
};

fn footprint(context: u64) -> Option<SlotFootprint> {
    SlotFootprint::new(
        EMBEDDER_WEIGHTS,
        Some(EMBEDDER_KV),
        KvCacheType::Q8_0,
        KvCacheType::Q8_0,
        context,
    )
}

/// Plenty of free VRAM, so the ceiling is the only thing that can refuse.
const ROOMY: Option<u64> = Some(8 * 1024 * 1024 * 1024);

/// What the second slot was sized for, and what it still does at the floor.
#[test]
fn at_the_old_floor_an_embedder_co_resides_comfortably() {
    let verdict = decide_secondary_slot(footprint(4096), ROOMY);
    assert!(
        verdict.is_grant(),
        "the slot exists for exactly this model: {verdict:?}"
    );
}

/// And what a fitted context does to it.
///
/// `fit_context` sizes against the whole device, so on a roomy card this model
/// reaches its trained ceiling — at which point its KV alone is about 2 GiB and
/// the footprint clears the slot's ceiling on its own.
#[test]
fn at_a_fitted_rung_the_same_embedder_is_refused_as_too_large() {
    let verdict = decide_secondary_slot(footprint(32_768), ROOMY);
    assert!(
        matches!(verdict, SecondarySlotDecision::RefuseTooLarge { .. }),
        "expected the ceiling to bind, got {verdict:?}"
    );
}

/// The rung where it stops fitting, so the cost is a number rather than an
/// adjective. Every rung at or below this one co-resides; the next one does not.
#[test]
fn the_ceiling_binds_between_the_8k_and_16k_rungs() {
    assert!(
        decide_secondary_slot(footprint(8_192), ROOMY).is_grant(),
        "8k must still fit"
    );
    assert!(
        decide_secondary_slot(footprint(16_384), ROOMY).is_grant(),
        "16k must still fit"
    );
    assert!(
        !decide_secondary_slot(footprint(32_768), ROOMY).is_grant(),
        "32k must not"
    );
}

/// The consequence, stated as the property that matters: a *larger* fitted
/// context makes a model *less* able to co-reside, so sizing a candidate
/// against the whole device is what disqualifies it from the smaller slot.
#[test]
fn a_larger_context_can_only_make_co_residence_harder() {
    let mut last_grant = true;
    for rung in [4096_u64, 8192, 16_384, 32_768, 65_536] {
        let grant = decide_secondary_slot(footprint(rung), ROOMY).is_grant();
        assert!(
            !grant || last_grant,
            "co-residence must not become possible again at {rung}"
        );
        last_grant = grant;
    }
}
