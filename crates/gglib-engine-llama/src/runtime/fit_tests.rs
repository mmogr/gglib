use gglib_core::domain::phone_fit::{FitVerdict, fit_verdict};
use gglib_core::ports::ModelRuntimeError;

use super::context_within;
use crate::runtime::test_support::{BUDGET, GIB, model};

/// The context a model of `weights` bytes, as [`model`] makes it, gets.
fn context(
    weights: u64,
    budget: Option<u64>,
    asked: Option<u64>,
) -> Result<u32, ModelRuntimeError> {
    context_within(&model(1, "m", weights), weights, budget, asked)
}

#[test]
fn a_model_gets_the_largest_rung_its_budget_fits() {
    assert_eq!(context(GIB, Some(BUDGET), None).unwrap(), 8192);
    // 1.5 GiB of weights leaves room for 4626 tokens: the lowest rung.
    assert_eq!(context(3 * GIB / 2, Some(BUDGET), None).unwrap(), 4096);
}

#[test]
fn a_context_asked_for_lowers_the_rung_and_never_raises_it() {
    assert_eq!(context(GIB, Some(BUDGET), Some(2048)).unwrap(), 2048);
    assert_eq!(context(GIB, Some(BUDGET), Some(65_536)).unwrap(), 8192);
    assert_eq!(context(GIB, Some(BUDGET), Some(0)).unwrap(), 8192);
}

#[test]
fn a_model_whose_smallest_rung_does_not_fit_is_refused_with_both_budgets() {
    let spec = model(1, "m", 3 * GIB);
    let (verdict, _) = fit_verdict(
        spec.context_length,
        Some(3 * GIB),
        spec.kv_elems_per_token,
        Some(BUDGET),
    );
    let FitVerdict::TooBig { need_bytes, .. } = verdict else {
        panic!("3 GiB of weights do not fit 2 GiB: {verdict:?}");
    };
    match context(3 * GIB, Some(BUDGET), None) {
        Err(ModelRuntimeError::ChatModelDoesNotFit {
            needed_bytes,
            budget_bytes,
        }) => {
            assert_eq!(budget_bytes, BUDGET);
            assert_eq!(needed_bytes, need_bytes);
            assert!(needed_bytes > budget_bytes);
        }
        other => panic!("refused as too big, not {other:?}"),
    }
}

#[test]
fn a_model_trained_under_the_lowest_rung_is_refused_naming_its_trained_context() {
    let mut short = model(1, "short", GIB);
    short.context_length = Some(2048);
    // Too big with room to spare: memory is not the reason.
    let (verdict, _) = fit_verdict(
        short.context_length,
        Some(GIB),
        short.kv_elems_per_token,
        Some(BUDGET),
    );
    assert!(
        matches!(verdict, FitVerdict::TooBig { need_bytes, have_bytes } if need_bytes <= have_bytes),
        "{verdict:?}"
    );
    match context_within(&short, GIB, Some(BUDGET), None) {
        Err(ModelRuntimeError::SpawnFailed(why)) => {
            assert!(why.contains("'short' was trained for 2048 tokens"), "{why}");
            assert!(why.contains("under the 4096"), "{why}");
        }
        other => panic!("refused naming its trained context, not {other:?}"),
    }
}

#[test]
fn a_fit_that_cannot_be_judged_is_refused_naming_what_is_missing() {
    let mut shapeless = model(1, "shapeless", GIB);
    shapeless.kv_elems_per_token = None;
    let refusals = [
        (
            context_within(&shapeless, GIB, Some(BUDGET), None),
            "KV cache",
        ),
        (context(GIB, None, None), "memory"),
        // A size that cannot be read is 0.
        (context(0, Some(BUDGET), None), "weights"),
    ];
    for (refused, missing) in refusals {
        match refused {
            Err(ModelRuntimeError::SpawnFailed(why)) => {
                assert!(why.contains("can't tell whether"), "{why}");
                assert!(why.contains(missing), "{why}");
            }
            other => panic!("refused naming {missing}, not {other:?}"),
        }
    }
}
