//! Table tests for [`super::fit_context`].

use super::{RUNGS, fit_context};
use crate::cache_config::KvCacheType;
use crate::domain::kv_estimate::KvElemsPerToken;
use crate::settings::DEFAULT_CONTEXT_SIZE;

const GIB: u64 = 1024 * 1024 * 1024;

/// Qwen3-4B-shaped: 8 KV heads × 128 dims × 36 layers ≈ 36 864 elems/token,
/// which at f16 is ~72 KiB per token of context.
const fn kv() -> KvElemsPerToken {
    KvElemsPerToken {
        k: 36_864,
        v: 36_864,
    }
}

fn fit(weights: u64, budget: u64, trained: u64) -> Option<u64> {
    fit_context(
        Some(trained),
        Some(weights),
        Some(kv()),
        KvCacheType::F16,
        KvCacheType::F16,
        Some(budget),
    )
}

#[test]
fn fit_is_capped_by_the_trained_context() {
    // Enormous budget, small trained window: the model's own limit wins.
    let fitted = fit(2 * GIB, 512 * GIB, 8192).expect("fits");
    assert_eq!(fitted, 8192);
}

#[test]
fn fit_is_capped_by_the_memory_budget() {
    // Same model, same trained window, less memory: the machine wins.
    let roomy = fit(2 * GIB, 64 * GIB, 131_072).expect("fits");
    let cramped = fit(2 * GIB, 8 * GIB, 131_072).expect("fits");
    assert!(
        cramped < roomy,
        "a smaller budget must yield a smaller context: {cramped} vs {roomy}"
    );
}

#[test]
fn fit_always_lands_on_a_rung() {
    for budget_gib in 3..=64 {
        if let Some(fitted) = fit(2 * GIB, budget_gib * GIB, 131_072) {
            assert!(
                RUNGS.contains(&fitted),
                "{fitted} (at {budget_gib} GiB) is not a rung"
            );
        }
    }
}

#[test]
fn a_slightly_larger_budget_does_not_move_the_answer() {
    // The churn guard: a resident is identified by its context, so a value
    // that drifted with a live memory reading would recycle the server on
    // essentially every request.
    let base = fit(2 * GIB, 24 * GIB, 131_072).expect("fits");
    for delta_mib in [1_u64, 8, 64, 256] {
        let nudged = fit(2 * GIB, 24 * GIB + delta_mib * 1024 * 1024, 131_072).expect("fits");
        assert_eq!(
            nudged, base,
            "a {delta_mib} MiB difference moved the context from {base} to {nudged}"
        );
    }
}

#[test]
fn fit_refuses_when_the_kv_shape_is_unknown() {
    assert_eq!(
        fit_context(
            Some(32_768),
            Some(2 * GIB),
            None,
            KvCacheType::F16,
            KvCacheType::F16,
            Some(64 * GIB)
        ),
        None
    );
}

#[test]
fn fit_refuses_when_the_trained_context_is_unknown() {
    assert_eq!(
        fit_context(
            None,
            Some(2 * GIB),
            Some(kv()),
            KvCacheType::F16,
            KvCacheType::F16,
            Some(64 * GIB)
        ),
        None
    );
}

#[test]
fn fit_refuses_when_no_memory_reading_is_available() {
    assert_eq!(
        fit_context(
            Some(32_768),
            Some(2 * GIB),
            Some(kv()),
            KvCacheType::F16,
            KvCacheType::F16,
            None
        ),
        None
    );
}

#[test]
fn fit_refuses_when_the_weights_alone_exceed_the_budget() {
    // Not a tiny context — no context. The built-in default takes over and
    // fails honestly rather than this module inventing a number.
    assert_eq!(fit(40 * GIB, 8 * GIB, 32_768), None);
}

#[test]
fn fit_refuses_when_it_cannot_reach_the_smallest_rung() {
    // Weights fit, but what is left over will not hold 4096 tokens of KV.
    let fitted = fit(8 * GIB - 1, 8 * GIB, 32_768);
    assert_eq!(fitted, None);
}

#[test]
fn a_fitted_value_is_never_below_the_built_in_default() {
    for budget_gib in 1..=128 {
        if let Some(fitted) = fit(2 * GIB, budget_gib * GIB, 131_072) {
            assert!(
                fitted >= DEFAULT_CONTEXT_SIZE,
                "{fitted} is below the floor"
            );
        }
    }
}

#[test]
fn quantized_kv_buys_more_context_than_f16() {
    // A budget small enough that neither arm hits the trained ceiling, or the
    // cap would mask the difference this test is about.
    let f16 = fit_context(
        Some(131_072),
        Some(2 * GIB),
        Some(kv()),
        KvCacheType::F16,
        KvCacheType::F16,
        Some(8 * GIB),
    )
    .expect("fits");
    let q8 = fit_context(
        Some(131_072),
        Some(2 * GIB),
        Some(kv()),
        KvCacheType::Q8_0,
        KvCacheType::Q8_0,
        Some(8 * GIB),
    )
    .expect("fits");
    assert!(q8 > f16, "q8_0 KV ({q8}) should beat f16 ({f16})");
}

#[test]
fn fit_refuses_when_the_weight_size_is_unknown() {
    // `0` is `total_model_bytes`' sentinel for "could not be read", and it is
    // the most dangerous value to take literally: weights that cost nothing
    // hand the whole budget to the KV cache. `launch_deadline_secs` refuses
    // the same field the same way.
    assert_eq!(
        fit_context(
            Some(32_768),
            Some(0),
            Some(kv()),
            KvCacheType::F16,
            KvCacheType::F16,
            Some(64 * GIB)
        ),
        None,
        "an unreadable weight size must not fit the largest possible context"
    );
    assert_eq!(
        fit_context(
            Some(32_768),
            None,
            Some(kv()),
            KvCacheType::F16,
            KvCacheType::F16,
            Some(64 * GIB)
        ),
        None
    );
}

/// The property the whole function exists to guarantee, and the one a
/// round-up snap would silently break: what it returns must actually fit.
#[test]
fn a_fitted_context_always_fits_the_budget_it_was_given() {
    let per_token =
        crate::domain::kv_estimate::kv_bytes_per_token(kv(), KvCacheType::F16, KvCacheType::F16);
    for budget_gib in 3..=200 {
        let budget = budget_gib * GIB;
        let weights = 2 * GIB;
        let Some(fitted) = fit(weights, budget, 131_072) else {
            continue;
        };
        let needed = weights + fitted * per_token;
        // Integer arithmetic: the reserve is a tenth, and a test that casts
        // through f64 to check a cast through f64 proves nothing.
        let allowed = budget - budget / 10;
        assert!(
            needed <= allowed,
            "at {budget_gib} GiB the fit chose {fitted}, needing {needed} of {allowed} allowed"
        );
    }
}

/// The utilisation reserve is load-bearing: without it a fit that "fits on
/// paper" spills into host memory and runs at a fraction of the speed.
///
/// Pinned to a concrete rung rather than compared against the constant — a
/// test that derives its expectation from the value under test moves with any
/// mutation of it and asserts nothing. At 12 GiB with these weights the
/// reserve is exactly what separates 32768 from 65536.
#[test]
fn the_fit_leaves_the_utilisation_reserve_unspent() {
    assert_eq!(
        fit(2 * GIB, 12 * GIB, 131_072),
        Some(32_768),
        "spending the reserve would reach the next rung up"
    );
}

/// A trained window that is not itself a rung must still never be exceeded.
#[test]
fn a_non_rung_trained_context_is_never_exceeded() {
    for trained in [6000_u64, 9001, 40_000, 100_000, 131_071] {
        let fitted = fit(2 * GIB, 512 * GIB, trained);
        if let Some(fitted) = fitted {
            assert!(
                fitted <= trained,
                "fitted {fitted} exceeds a trained window of {trained}"
            );
            assert!(RUNGS.contains(&fitted), "{fitted} is not a rung");
        }
    }
}

/// Below the smallest rung there is no honest answer, and inventing one is
/// worse than falling through to the built-in default.
#[test]
fn a_trained_window_under_the_smallest_rung_refuses() {
    assert_eq!(fit(2 * GIB, 512 * GIB, 2048), None);
}
