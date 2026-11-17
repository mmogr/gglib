//! Tests for the load's fit: the reservation and its fallback.

use super::*;
use crate::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V};
use crate::domain::kv_estimate::KvElemsPerToken;
use crate::domain::residency::{SlotFootprint, decide_secondary_slot};

const GIB: u64 = 1024 * 1024 * 1024;

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
        withheld < SECONDARY_MAX_BYTES,
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
    let need = scale_up(SECONDARY_MAX_BYTES, RESIDENCY_UTILISATION);
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

/// A secondary at the ceiling the reservation is sized for is admitted, and
/// one a byte over is not: the reservation and the admission read one
/// constant.
#[test]
fn the_reserve_is_the_secondary_ceiling() {
    let free = 64 * GIB;
    let weights_only = |bytes| {
        SlotFootprint::new(
            bytes,
            Some(KvShape::uniform(KvElemsPerToken { k: 0, v: 0 })),
            KvCacheType::F16,
            KvCacheType::F16,
            0,
        )
        .expect("known kv shape")
    };
    assert!(decide_secondary_slot(Some(weights_only(SECONDARY_MAX_BYTES)), Some(free)).is_grant());
    assert!(
        !decide_secondary_slot(Some(weights_only(SECONDARY_MAX_BYTES + 1)), Some(free)).is_grant()
    );
}

/// The fallback: when the co-resident reservation leaves too little to fit at
/// all, the model is sized against the undivided device rather than dropping
/// to the built-in floor.
///
/// That machine is precisely the one where a full-ceiling secondary could
/// never have loaded, so the reservation buys nothing and costs everything.
#[test]
fn a_budget_too_small_to_fit_falls_back_to_the_undivided_device() {
    // Stands in for `fit_context`: refuses the smaller budget, fits the larger.
    let fit = |budget: Option<u64>| budget.filter(|&b| b >= 6_000).map(|_| 32_768);

    assert_eq!(
        fit_or_undivided(fit, Some(4_000), Some(6_000)),
        Some(32_768),
        "a reservation that refuses must fall through to the undivided device"
    );
}

/// The reserved budget wins whenever it fits — the fallback is an escape
/// hatch, not a preference.
#[test]
fn the_reserved_budget_is_used_whenever_it_fits() {
    let fit = |budget: Option<u64>| budget.map(|b| b / 1000);

    assert_eq!(
        fit_or_undivided(fit, Some(6_000), Some(24_000)),
        Some(6),
        "the undivided device must not be consulted when the reservation fits"
    );
}

/// 28 layers of 4 KV heads of 128: Qwen2.5-Coder-7B's cache.
const fn qwen_7b() -> KvShape {
    KvShape::uniform(KvElemsPerToken {
        k: 28 * 4 * 128,
        v: 28 * 4 * 128,
    })
}

/// The verdict is the fit's, at the default KV types, and its inputs are
/// those of the budget that answered: a size that fits the reserved budget
/// is judged there, and one that does not, against the whole device.
#[test]
fn the_verdict_is_the_context_the_load_would_serve() {
    let device = 8 * GIB;
    let fit_at = |weights| {
        fit_on_device(
            Some(32_768),
            Some(weights),
            Some(qwen_7b()),
            DEFAULT_CACHE_TYPE_K,
            DEFAULT_CACHE_TYPE_V,
            Some(device),
        )
    };
    for weights in [3 * GIB, 5_781_197_600, 6_254_199_584, 8 * GIB] {
        let (fitted, inputs) = fit_at(weights);
        let (verdict, judged) = device_fit_verdict(
            Some(32_768),
            Some(weights),
            Some(qwen_7b()),
            DEFAULT_CACHE_TYPE_K,
            DEFAULT_CACHE_TYPE_V,
            Some(device),
        );
        assert_eq!(judged.budget_bytes, inputs.budget_bytes, "{weights}");
        let expected = match fitted {
            Some(ctx) if ctx > 4096 => FitVerdict::Fits { ctx },
            Some(ctx) => FitVerdict::Tight { ctx },
            None => verdict,
        };
        assert_eq!(verdict, expected, "{weights}");
    }
    // Q5_K_L fits the reserved budget, at 8K; Q6_K does not, and falls back
    // to the whole card at 32K.
    assert_eq!(fit_at(5_781_197_600).0, Some(8192));
    assert_eq!(
        fit_at(5_781_197_600).1.budget_bytes,
        Some(fit_budget_from(device))
    );
    assert_eq!(fit_at(6_254_199_584).0, Some(32_768));
    assert_eq!(fit_at(6_254_199_584).1.budget_bytes, Some(device));
    assert!(matches!(
        device_fit_verdict(
            Some(32_768),
            Some(8 * GIB),
            Some(qwen_7b()),
            DEFAULT_CACHE_TYPE_K,
            DEFAULT_CACHE_TYPE_V,
            Some(device)
        )
        .0,
        FitVerdict::TooBig { .. }
    ));
}
