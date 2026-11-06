//! Unit tests for [`super::explain_fit`].
//!
//! The device probe is process-global and cached, so these assert the parts
//! that do not depend on it: a refusal names its reason, and the inputs
//! describe the budget that actually answered.

use super::*;
use gglib_core::cache_config::KvCacheType;

/// A refusal is not a zero, and `FitInputs` says which refusal it was. Without
/// this the only signal is a `None`, which sends a person reading source.
#[test]
fn a_refusal_still_reports_what_it_knew() {
    // No trained window: nothing else can rescue it.
    let (fitted, inputs) = explain_fit(
        None,
        Some(4_000_000_000),
        None,
        KvCacheType::F16,
        KvCacheType::F16,
    );
    assert_eq!(fitted, None, "an unknown trained window must refuse");
    assert_eq!(inputs.trained_ctx, None);
    assert_eq!(inputs.unsnapped, None, "nothing could be computed to snap");
}

/// The zero sentinel is the dangerous one: weights that cost nothing would hand
/// the whole budget to the KV cache. `FitInputs` must report it as unknown
/// rather than as a real zero.
#[test]
fn zero_weights_are_reported_as_unknown_not_as_free() {
    let (fitted, inputs) = explain_fit(
        Some(32_768),
        Some(0),
        None,
        KvCacheType::F16,
        KvCacheType::F16,
    );
    assert_eq!(fitted, None);
    assert_eq!(
        inputs.weights_bytes, None,
        "0 is the could-not-read sentinel, not a weightless model"
    );
}

/// `explain_fit` reports the same rung the launch path would take, because it
/// goes through the same seam with the same two budgets. This asserts the
/// agreement rather than the value, which depends on the machine.
#[test]
fn the_explanation_agrees_with_the_budget_it_reports() {
    let kv = KvElemsPerToken { k: 4096, v: 4096 };
    let (fitted, inputs) = explain_fit(
        Some(131_072),
        Some(4_000_000_000),
        Some(kv),
        KvCacheType::Q8_0,
        KvCacheType::Q8_0,
    );
    let direct = gglib_core::domain::fit_context(
        Some(131_072),
        Some(4_000_000_000),
        Some(kv),
        KvCacheType::Q8_0,
        KvCacheType::Q8_0,
        inputs.budget_bytes,
    );
    assert_eq!(
        fitted, direct,
        "the reported budget must be the one that produced the rung"
    );
}

/// What a launch would do to what is loaded, apart from any machine's VRAM:
/// nothing in the primary unloads nothing; a refusal of the second slot swaps
/// the primary's resident out and names it; a grant loads beside an empty
/// second slot, and beside a full one unloads one of them, unnamed.
#[test]
fn a_plan_says_what_it_would_unload() {
    use gglib_core::domain::{CacheRamHealth, ModelSamplingDefaults, SecondarySlotDecision};
    let resident = |model_id: u32, name: &str| Resident {
        model_sampling: ModelSamplingDefaults::default(),
        model_id,
        model_name: name.to_owned(),
        context_size: 4096,
        port: 1,
        projector: None,
        runtime: RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: tokio::time::Instant::now(),
        weights_bytes: 1,
    };
    let grant = SecondarySlotDecision::Grant {
        footprint_bytes: 1,
        headroom_bytes: 1,
    };
    let refusal = SecondarySlotDecision::RefuseUnknownFootprint;
    let gemma = resident(7, "gemma");
    let nomic = resident(9, "nomic");

    assert_eq!(displacement(&refusal, None, &[]), (false, None));
    assert_eq!(displacement(&grant, None, &[nomic.clone()]), (false, None));
    assert_eq!(
        displacement(&refusal, Some(&gemma), &[]),
        (true, Some("gemma".to_owned()))
    );
    assert_eq!(displacement(&grant, Some(&gemma), &[]), (false, None));
    assert_eq!(displacement(&grant, Some(&gemma), &[nomic]), (true, None));
}
