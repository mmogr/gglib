//! Unit tests for [`super::explain_fit`].
//!
//! The device probe is process-global and cached, so these assert the parts
//! that do not depend on it: a refusal names its reason, and the inputs
//! describe the budget that actually answered.

use super::*;
use gglib_core::cache_config::KvCacheType;
use gglib_core::domain::KvElemsPerToken;

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
    let kv = KvShape::uniform(KvElemsPerToken { k: 4096, v: 4096 });
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

/// The residents and decisions the unload verdicts are read against.
fn unload_fixtures() -> (
    Resident,
    Resident,
    Resident,
    gglib_core::domain::SecondarySlotDecision,
    gglib_core::domain::SecondarySlotDecision,
) {
    use gglib_core::domain::{CacheRamHealth, ModelSamplingDefaults, SecondarySlotDecision};
    let resident = |model_id: u32, name: &str, runtime: RuntimeKind, weights_bytes: u64| Resident {
        model_sampling: ModelSamplingDefaults::default(),
        model_id,
        model_name: name.to_owned(),
        context_size: 4096,
        port: 1,
        projector: None,
        runtime,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: tokio::time::Instant::now(),
        weights_bytes,
    };
    let grant = SecondarySlotDecision::Grant {
        footprint_bytes: 1,
        headroom_bytes: 1,
    };
    let refusal = SecondarySlotDecision::RefuseUnknownFootprint;
    (
        resident(7, "qwen", RuntimeKind::Llama, GIB),
        resident(9, "nomic", RuntimeKind::Llama, GIB),
        // Three GiB: an image model's place gives back more than a chat
        // model's, which the place-fit cases tell apart.
        resident(11, "flux-a", RuntimeKind::StableDiffusion, 3 * GIB),
        grant,
        refusal,
    )
}

const GIB: u64 = 1024 * 1024 * 1024;

fn unloads(model: &str, reason: UnloadReason) -> Unload {
    Unload::new(model.to_owned(), reason)
}

/// What a chat plan would unload, by placement's rules, apart from any
/// machine's VRAM: nothing with the primary empty; refused the second slot,
/// the primary's resident; granted it, the second slot's resident if one is
/// there, else nothing; and nothing for a model already loaded in either
/// slot, which relaunches in place.
#[test]
fn a_chat_plan_says_what_it_would_unload() {
    let (qwen, nomic, _flux, grant, refusal) = unload_fixtures();
    let chat = RuntimeKind::Llama;
    let one = std::slice::from_ref(&nomic);
    assert_eq!(displacement(chat, &refusal, None, &[], false), None);
    assert_eq!(displacement(chat, &grant, None, one, false), None);
    assert_eq!(
        displacement(chat, &refusal, Some(&qwen), &[], false),
        Some(unloads("qwen", UnloadReason::WouldNotFitBeside))
    );
    assert_eq!(
        displacement(chat, &refusal, Some(&qwen), one, false),
        Some(unloads("qwen", UnloadReason::WouldNotFitBeside))
    );
    assert_eq!(displacement(chat, &grant, Some(&qwen), &[], false), None);
    assert_eq!(
        displacement(chat, &grant, Some(&qwen), one, false),
        Some(unloads("nomic", UnloadReason::SecondSlotFull))
    );
    // Already loaded, in either slot: a relaunch in place.
    assert_eq!(displacement(chat, &refusal, Some(&qwen), &[], true), None);
    assert_eq!(displacement(chat, &refusal, Some(&qwen), one, true), None);
}

/// What an image plan would unload, by placement's rules: the loaded image
/// model, whichever slot it is in, when the newcomer fits in its place; the
/// second slot's resident only when granted the slot or with the primary
/// empty; else the primary's resident, as a refused image model beside a
/// chat model is swapped into the primary. One too large for the only place
/// it may take waits, and nothing is named.
#[test]
fn an_image_plan_says_what_it_would_unload() {
    use gglib_core::domain::SecondarySlotDecision;
    let (qwen, nomic, flux, grant, refusal) = unload_fixtures();
    let image = RuntimeKind::StableDiffusion;
    let nomic_only = std::slice::from_ref(&nomic);
    let flux_only = std::slice::from_ref(&flux);

    // Nothing loaded: an image model alone is placed whatever the budget.
    assert_eq!(displacement(image, &refusal, None, &[], false), None);

    // The loaded image model's slot, whichever slot that is, on a grant or a
    // refusal whose budget is unknown.
    for decision in [&grant, &refusal] {
        assert_eq!(
            displacement(image, decision, Some(&qwen), flux_only, false),
            Some(unloads("flux-a", UnloadReason::OneImageModel))
        );
        assert_eq!(
            displacement(image, decision, Some(&flux), nomic_only, false),
            Some(unloads("flux-a", UnloadReason::OneImageModel))
        );
        assert_eq!(
            displacement(image, decision, Some(&flux), &[], false),
            Some(unloads("flux-a", UnloadReason::OneImageModel))
        );
    }

    // No image model loaded, beside a chat primary: granted the second slot,
    // it takes it free or from its resident; refused it, the primary's
    // resident is swapped out, whatever the second slot holds.
    assert_eq!(displacement(image, &grant, Some(&qwen), &[], false), None);
    assert_eq!(
        displacement(image, &grant, Some(&qwen), nomic_only, false),
        Some(unloads("nomic", UnloadReason::SecondSlotFull))
    );
    assert_eq!(
        displacement(image, &refusal, Some(&qwen), &[], false),
        Some(unloads("qwen", UnloadReason::WouldNotFitBeside))
    );
    assert_eq!(
        displacement(image, &refusal, Some(&qwen), nomic_only, false),
        Some(unloads("qwen", UnloadReason::WouldNotFitBeside))
    );
    // The primary empty: the second slot is open to it, free or held.
    assert_eq!(
        displacement(image, &refusal, None, nomic_only, false),
        Some(unloads("nomic", UnloadReason::SecondSlotFull))
    );

    // Too large for the only place it may take, judged with that model's
    // weights given back: it waits, and nothing is named. Beside a chat
    // primary of one GiB with two GiB free, a four GiB image model does not
    // fit in the primary's place; a one GiB one does.
    let too_large = SecondarySlotDecision::RefuseNoHeadroom {
        footprint_bytes: 4 * GIB,
        free_bytes: 2 * GIB,
    };
    assert_eq!(
        displacement(image, &too_large, Some(&qwen), &[], false),
        None
    );
    // Beside flux the place is flux's slot, and flux's three GiB given back
    // (five GiB free, 4.5 usable) take the four GiB model: flux goes. The
    // same model judged against qwen's one GiB would wait.
    assert_eq!(
        displacement(image, &too_large, Some(&qwen), flux_only, false),
        Some(unloads("flux-a", UnloadReason::OneImageModel))
    );
    let too_large_for_flux = SecondarySlotDecision::RefuseNoHeadroom {
        footprint_bytes: 8 * GIB,
        free_bytes: 2 * GIB,
    };
    assert_eq!(
        displacement(image, &too_large_for_flux, Some(&qwen), flux_only, false),
        None
    );
    // An image model loaded alone is replaced whatever the budget, as on a
    // cold daemon: the place is judged only while another model is loaded.
    assert_eq!(
        displacement(image, &too_large_for_flux, Some(&flux), &[], false),
        Some(unloads("flux-a", UnloadReason::OneImageModel))
    );
    assert_eq!(
        displacement(image, &too_large_for_flux, None, flux_only, false),
        Some(unloads("flux-a", UnloadReason::OneImageModel))
    );
    let fits_in_place = SecondarySlotDecision::RefuseNoHeadroom {
        footprint_bytes: GIB,
        free_bytes: GIB,
    };
    assert_eq!(
        displacement(image, &fits_in_place, Some(&qwen), &[], false),
        Some(unloads("qwen", UnloadReason::WouldNotFitBeside))
    );

    // Already loaded: a relaunch in place.
    assert_eq!(
        displacement(image, &refusal, Some(&qwen), flux_only, true),
        None
    );
}

/// A chat model with a trained window, weights and a KV cost, so the fit
/// has every figure it needs, whatever this machine's budget.
fn sized_spec() -> ModelLaunchSpec {
    let mut spec = crate::process::residency::residency_tests::launch_spec(5, "qwen");
    spec.context_length = Some(131_072);
    spec.file_size_bytes = 4_000_000_000;
    spec.kv_shape = Some(KvShape::uniform(KvElemsPerToken { k: 4096, v: 4096 }));
    spec
}

/// A launch's narration carries the fit its context was sized by: the
/// value a plan of it shows, the rung the options resolved with, and the
/// context and the chain's rung the launch serves at. Equalities, not
/// figures, since the budget is this machine's.
#[test]
fn a_launch_narration_carries_the_fit_its_context_was_sized_by() {
    let request = super::super::prepare(
        sized_spec(),
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        None,
        &LaunchOverrides::default(),
    );
    let planned = plan_llama(&[], &request);
    let fit = planned
        .narration
        .fit
        .expect("a chat model's narration has its fit");
    assert_eq!(Some(&fit), request.fit.as_ref());
    assert_eq!(fit.fitted, request.opts.fitted_ctx);
    assert_eq!((fit.serves_at, fit.source), request.context);
    assert_eq!(fit.weights_bytes, Some(4_000_000_000));
}

/// A plan and the launch it describes show one fit, read twice.
#[test]
fn a_plan_shows_the_fit_its_narration_carries() {
    let plan = plan_launch(
        sized_spec(),
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        None,
        &LaunchOverrides::default(),
        Loaded::NOTHING,
    )
    .expect("a chat model plans");
    assert!(plan.fit.is_some());
    assert_eq!(plan.fit, plan.narration.fit);
}

/// An image model has no context, so its launch carries no fit.
#[test]
fn an_image_model_carries_no_fit() {
    let mut spec = crate::process::residency::residency_tests::launch_spec(6, "flux");
    spec.image_family = Some(gglib_core::domain::ImageFamily::Flux1);
    assert_eq!(spec.runtime(), RuntimeKind::StableDiffusion);
    let request = super::super::prepare(
        spec,
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        None,
        &LaunchOverrides::default(),
    );
    assert_eq!(request.fit, None);
}
