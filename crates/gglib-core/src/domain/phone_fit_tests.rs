//! Boundary tests for [`super::fit_verdict`].

use super::{FitVerdict, LOWEST_RUNG, MissingInput, fit_verdict};
use crate::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V};
use crate::domain::context_fit::fit_context;
use crate::domain::kv_estimate::{KvElemsPerToken, kv_bytes_per_token};
use crate::domain::recommendation::{budget_covering, usable_share};

const GIB: u64 = 1024 * 1024 * 1024;

/// `unsloth/Qwen3-4B-GGUF` `Q4_K_M`: the file's size on the Hub, and 36
/// layers x 8 KV heads x 128 head dim from its own metadata.
const WEIGHTS: u64 = 2_497_281_312;
const TRAINED: u64 = 40_960;
const KV: KvElemsPerToken = KvElemsPerToken {
    k: 36_864,
    v: 36_864,
};

fn per_token() -> u64 {
    kv_bytes_per_token(KV, DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V)
}

/// Weights plus the lowest rung's KV cache: what the lowest rung occupies.
fn lowest_rung_bytes() -> u64 {
    WEIGHTS + LOWEST_RUNG * per_token()
}

/// The smallest budget the lowest rung fits in: what a verdict calls
/// `need_bytes`.
fn need() -> u64 {
    budget_covering(lowest_rung_bytes())
}

fn verdict_at(budget: u64) -> FitVerdict {
    fit_verdict(Some(TRAINED), Some(WEIGHTS), Some(KV), Some(budget)).0
}

#[test]
fn a_rung_above_the_lowest_fits() {
    assert_eq!(verdict_at(8 * GIB), FitVerdict::Fits { ctx: 32_768 });
    // The second rung's own edge: the smallest budget a Fits verdict needs.
    let second = budget_covering(WEIGHTS + 2 * LOWEST_RUNG * per_token());
    assert_eq!(verdict_at(second), FitVerdict::Fits { ctx: 8192 });
}

#[test]
fn only_the_lowest_rung_is_tight() {
    let between = budget_covering(WEIGHTS + 6000 * per_token());
    assert_eq!(verdict_at(between), FitVerdict::Tight { ctx: LOWEST_RUNG });
    // A byte short of the second rung is still tight.
    let short_of_second = budget_covering(WEIGHTS + 2 * LOWEST_RUNG * per_token()) - 1;
    assert_eq!(
        verdict_at(short_of_second),
        FitVerdict::Tight { ctx: LOWEST_RUNG }
    );
}

/// A model trained for fewer tokens than the second rung is held at the
/// lowest however much memory there is, and the inputs say why.
#[test]
fn a_short_trained_window_is_tight_with_memory_to_spare() {
    let (verdict, inputs) = fit_verdict(Some(6000), Some(WEIGHTS), Some(KV), Some(64 * GIB));
    assert_eq!(verdict, FitVerdict::Tight { ctx: LOWEST_RUNG });
    assert_eq!(inputs.trained_ctx, Some(6000));
}

/// The boundary between tight and too big is the usable share of the budget
/// against the weights plus the lowest rung's KV cache, to the byte, and
/// `need_bytes` is the first budget on the tight side of it: a model too big
/// for the memory always needs more than it has.
#[test]
fn too_big_exactly_when_the_usable_share_is_short_of_the_lowest_rung() {
    let edge = need();
    let (mut tight, mut too_big) = (0, 0);
    for budget in edge - 64..=edge + 64 {
        match verdict_at(budget) {
            FitVerdict::TooBig {
                need_bytes,
                have_bytes,
            } => {
                assert!(
                    usable_share(budget) < lowest_rung_bytes(),
                    "{budget}: too big with room"
                );
                assert_eq!(need_bytes, edge);
                assert_eq!(have_bytes, budget);
                assert!(
                    need_bytes > have_bytes,
                    "{budget}: needs no more than it has"
                );
                too_big += 1;
            }
            FitVerdict::Tight { ctx } => {
                assert!(
                    usable_share(budget) >= lowest_rung_bytes(),
                    "{budget}: tight without room"
                );
                assert_eq!(ctx, LOWEST_RUNG);
                tight += 1;
            }
            other => panic!("{budget}: {other:?} at the lowest rung's edge"),
        }
    }
    // Too big below `need_bytes` and tight from it on, with no byte between.
    assert_eq!((too_big, tight), (64, 65));
}

#[test]
fn weights_beyond_the_budget_are_too_big() {
    assert_eq!(
        verdict_at(2 * GIB),
        FitVerdict::TooBig {
            need_bytes: need(),
            have_bytes: 2 * GIB,
        }
    );
    assert!(need() > 2 * GIB);
}

/// A budget a little over the weights and the lowest rung's KV cache is still
/// short once the fit's reserve is held back, and the verdict says so in one
/// kind of figure: the budget it needs is more than the budget it has.
#[test]
fn the_reserve_is_counted_in_what_a_model_needs() {
    let budget = lowest_rung_bytes() + lowest_rung_bytes() / 20;
    let FitVerdict::TooBig {
        need_bytes,
        have_bytes,
    } = verdict_at(budget)
    else {
        panic!("inside the reserve must be too big");
    };
    assert_eq!(have_bytes, budget);
    assert!(need_bytes > have_bytes);
}

/// The fit never invents a context below the ladder, so a model trained for
/// fewer tokens than the lowest rung is refused whatever the budget. Memory is
/// not the reason, so it needs less than it has; the inputs carry the reason.
#[test]
fn a_window_below_the_ladder_is_too_big_whatever_the_memory() {
    let (verdict, inputs) = fit_verdict(Some(2048), Some(WEIGHTS), Some(KV), Some(64 * GIB));
    assert_eq!(
        verdict,
        FitVerdict::TooBig {
            need_bytes: need(),
            have_bytes: 64 * GIB,
        }
    );
    assert!(need() < 64 * GIB);
    assert!(inputs.trained_ctx.is_some_and(|t| t < LOWEST_RUNG));
}

#[test]
fn each_missing_input_is_named() {
    let cases = [
        (
            fit_verdict(None, Some(WEIGHTS), Some(KV), Some(8 * GIB)),
            MissingInput::TrainedContext,
        ),
        (
            fit_verdict(Some(TRAINED), Some(WEIGHTS), None, Some(8 * GIB)),
            MissingInput::KvShape,
        ),
        (
            fit_verdict(Some(TRAINED), None, Some(KV), Some(8 * GIB)),
            MissingInput::Weights,
        ),
        (
            fit_verdict(Some(TRAINED), Some(WEIGHTS), Some(KV), None),
            MissingInput::Budget,
        ),
    ];
    for ((verdict, _), missing) in cases {
        assert_eq!(verdict, FitVerdict::Unknown { missing });
    }
}

/// Zero is the codebase's "could not be read", and the fit refuses it; the
/// verdict must name it as missing rather than call the model too big.
#[test]
fn zero_weights_and_a_costless_kv_shape_are_missing() {
    let zero_kv = KvElemsPerToken { k: 0, v: 0 };
    assert_eq!(
        fit_verdict(Some(TRAINED), Some(0), Some(KV), Some(8 * GIB)).0,
        FitVerdict::Unknown {
            missing: MissingInput::Weights
        }
    );
    assert_eq!(
        fit_verdict(Some(TRAINED), Some(WEIGHTS), Some(zero_kv), Some(8 * GIB)).0,
        FitVerdict::Unknown {
            missing: MissingInput::KvShape
        }
    );
}

#[test]
fn the_first_missing_input_is_the_one_named() {
    assert_eq!(
        fit_verdict(None, None, None, None).0,
        FitVerdict::Unknown {
            missing: MissingInput::TrainedContext
        }
    );
    assert_eq!(
        fit_verdict(Some(TRAINED), None, None, None).0,
        FitVerdict::Unknown {
            missing: MissingInput::KvShape
        }
    );
    assert_eq!(
        fit_verdict(Some(TRAINED), None, Some(KV), None).0,
        FitVerdict::Unknown {
            missing: MissingInput::Weights
        }
    );
}

/// The verdict is the fit read off, never a second calculation: across a sweep
/// of budgets its context is the fit's rung, and too big is the fit's refusal.
#[test]
fn the_verdict_agrees_with_the_fit_at_every_budget() {
    for budget in (GIB..=16 * GIB).step_by(64 * 1024 * 1024) {
        let fitted = fit_context(
            Some(TRAINED),
            Some(WEIGHTS),
            Some(KV),
            DEFAULT_CACHE_TYPE_K,
            DEFAULT_CACHE_TYPE_V,
            Some(budget),
        );
        let ctx = match verdict_at(budget) {
            FitVerdict::Fits { ctx } | FitVerdict::Tight { ctx } => Some(ctx),
            FitVerdict::TooBig { .. } => None,
            FitVerdict::Unknown { missing } => panic!("{budget}: {missing:?} with every input"),
        };
        assert_eq!(ctx, fitted, "at {budget} bytes");
    }
}

#[test]
fn the_inputs_are_returned_as_the_why() {
    let (_, inputs) = fit_verdict(Some(TRAINED), Some(WEIGHTS), Some(KV), Some(8 * GIB));
    assert_eq!(inputs.trained_ctx, Some(TRAINED));
    assert_eq!(inputs.weights_bytes, Some(WEIGHTS));
    assert_eq!(inputs.kv_bytes_per_token, Some(per_token()));
    assert_eq!(inputs.budget_bytes, Some(8 * GIB));
}
