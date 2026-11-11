//! The fit this machine would compute, without launching anything.
//!
//! `gglib model explain` answers "what would happen", and ADR 0009's first kill
//! criterion needs the same answer across a catalog: if the chosen rung is
//! routinely far below the unsnapped figure, the ladder is too coarse. Until
//! now the only record of that was a `debug!` line inside `admit`, emitted
//! after a launch and read by nothing.
//!
//! This is the same call `admit` makes — the same reserved budget, the same
//! fallback to the undivided device — so an explanation cannot describe a fit
//! that differs from the one a launch performs. It is deliberately *not* a
//! second implementation: `fit_or_undivided` is the shared seam, and this
//! module supplies the same two budgets to it.

use gglib_core::cache_config::{CacheRamSetting, KvCacheType};
use gglib_core::domain::{
    ContextFit, FitInputs, KvShape, LaunchPlan, RuntimeKind, SecondarySlotDecision, SlotFootprint,
    Unload, UnloadReason, decide_secondary_slot_for, fit_context_explained,
};
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec, ModelRuntimeError};
use gglib_core::server_config::ServerConfigOptions;

use super::context::fit_or_undivided;
use super::launch::{LaunchRequest, LlamaResolutions, plan_llama, plan_sd};
use super::{Prepared, explain_args, prepare, vram};
use crate::process::SpawnConfig;
use crate::process::admission::Resident;

/// What this machine would fit for a model, and what that answer worked from.
///
/// `None` for the fit is a refusal, not a zero: an unknown trained window, an
/// unreadable device, an unknown KV shape, or weights that already exceed the
/// budget. [`FitInputs`] says which, because a refusal with no reason is the
/// thing that sends a person reading source.
#[must_use]
pub fn explain_fit(
    trained_ctx: Option<u64>,
    weights_bytes: Option<u64>,
    kv: Option<KvShape>,
    k: KvCacheType,
    v: KvCacheType,
) -> (Option<u64>, FitInputs) {
    // Captured through the seam rather than beside it. `fit_or_undivided`
    // tries the reserved budget and falls back to the undivided device, so the
    // *last* closure call is always the one that answered — and running the
    // arithmetic a second time out here would be a copy of the fallback that
    // no test could tell had drifted. `FitInputs` is `Copy`, so a `Cell` is
    // enough to carry it back out of an `Fn`.
    let captured = std::cell::Cell::new(None);
    let fitted = fit_or_undivided(
        |budget| {
            let (rung, inputs) =
                fit_context_explained(trained_ctx, weights_bytes, kv, k, v, budget);
            captured.set(Some(inputs));
            rung
        },
        vram::fit_budget_for(),
        crate::system::total_device_memory_bytes(),
    );
    let inputs = captured
        .into_inner()
        .expect("fit_or_undivided calls the closure at least once");
    (fitted, inputs)
}

/// What is loaded when a plan is made, which its verdict on unloading is
/// read against.
///
/// The primary slot's resident, whatever it is; the other slots' residents,
/// which the RAM cache is sized after; and whether the planned model is
/// already loaded in any slot, when it relaunches in place and displaces
/// nothing. A one-shot command that knows nothing of the daemon's slots
/// passes [`Loaded::NOTHING`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Loaded<'a> {
    /// The model in the primary slot.
    pub primary: Option<&'a Resident>,
    /// The residents of the other slots.
    pub others: &'a [Resident],
    /// Whether the planned model is already loaded in some slot.
    pub already_resident: bool,
}

impl Loaded<'static> {
    /// Nothing loaded, or nothing known: the plan unloads nothing.
    pub const NOTHING: Self = Self {
        primary: None,
        others: &[],
        already_resident: false,
    };
}

/// The launch gglib would make for `spec` now, without making it.
///
/// The same resolutions a launch runs, in the same order (`prepare`, then
/// the runtime's planner), over `template` and this request's overrides
/// and the `others` loaded now, which the RAM cache is sized after. The
/// arguments are the spawn's own token list with the port spelled `auto`,
/// each with the decision behind it (`explain_args`); the fit is what the
/// context was sized by. Planned as a launch into the primary slot: the
/// queue chooses the slot when the launch happens, and a second slot's
/// cache differs.
///
/// Nothing is checked on disk: a file that is missing is the inspector's
/// to name, and a plan of a model whose weights are gone is still the plan.
///
/// # Errors
///
/// As a launch's planning: an image model with no family.
pub fn plan_launch(
    spec: ModelLaunchSpec,
    template: &ServerConfigOptions,
    cache_ram: CacheRamSetting,
    num_ctx: Option<u64>,
    default_ctx: Option<u64>,
    overrides: &LaunchOverrides,
    loaded: Loaded<'_>,
) -> Result<LaunchPlan, ModelRuntimeError> {
    let Loaded {
        primary,
        others,
        already_resident,
    } = loaded;
    let Prepared { request, fit } =
        prepare(spec, template, cache_ram, num_ctx, default_ctx, overrides);
    let planned = match request.spec.runtime() {
        RuntimeKind::Llama => plan_llama(others, &request),
        RuntimeKind::StableDiffusion => plan_sd(&request)?,
    };
    let (program, args) = match (&planned.config, &planned.llama) {
        (SpawnConfig::Llama(config), Some(resolved)) => (
            RuntimeKind::Llama.server_name(),
            explain_args::llama_args(config, resolved, &request),
        ),
        (SpawnConfig::Sd(config), _) => (
            RuntimeKind::StableDiffusion.server_name(),
            explain_args::sd_args(config),
        ),
        (SpawnConfig::Llama(_), None) => {
            return Err(ModelRuntimeError::Internal(
                "a llama-server plan carried no resolutions".to_owned(),
            ));
        }
    };
    let (serves_at, source) = request.context;
    let decision = second_slot_decision(&request, planned.llama.as_ref());
    let unloads = displacement(
        request.spec.runtime(),
        &decision,
        primary,
        others,
        already_resident,
    );
    Ok(LaunchPlan {
        program: program.to_owned(),
        args,
        narration: planned.narration,
        unloads,
        fit: fit.map(|(fitted, inputs)| ContextFit::new(serves_at, source, fitted, &inputs)),
    })
}

/// The second-slot decision this launch would meet: the same arithmetic
/// admission uses (`vram::secondary_slot_decision`), from the plan's own
/// resolutions. A llama plan with no resolutions has no footprint to judge.
fn second_slot_decision(
    request: &LaunchRequest,
    llama: Option<&LlamaResolutions>,
) -> SecondarySlotDecision {
    match (request.spec.runtime(), llama) {
        (RuntimeKind::Llama, Some(resolved)) => {
            vram::secondary_slot_decision(&request.spec, resolved.kv_types, request.context.0)
        }
        (RuntimeKind::StableDiffusion, _) => vram::image_slot_decision(&request.spec),
        (RuntimeKind::Llama, None) => SecondarySlotDecision::RefuseUnknownFootprint,
    }
}

/// The loaded model the launch would unload first, and why, from the
/// second-slot `decision` read against what is resident now, by the rules
/// placement follows (`QueueState::choose_slot`, `choose_image_slot`,
/// `may_use`, `too_large_for_its_place`). The plan sees residents only: a
/// slot still loading is the queue's to see.
///
/// `primary` is the model in the primary slot and `others` the other slots'
/// residents (one slot: `SLOT_COUNT` is two). A model already resident, in
/// either, relaunches in place and unloads nothing.
///
/// A chat model refused the second slot swaps the primary's resident out;
/// granted it, it sits beside the primary, and a model already in that slot
/// gives way first. An image model takes the loaded image model's slot,
/// whichever it is, when it fits in that model's place; else it may use the
/// second slot only when granted it or when the primary is empty, taking it
/// free or from its resident; else it swaps the primary's resident out. One
/// too large for the only place it may take waits, and nothing is named.
fn displacement(
    runtime: RuntimeKind,
    decision: &SecondarySlotDecision,
    primary: Option<&Resident>,
    others: &[Resident],
    already_resident: bool,
) -> Option<Unload> {
    if already_resident {
        return None;
    }
    let unload = |r: &Resident, reason: UnloadReason| {
        Some(Unload {
            model: r.model_name.clone(),
            reason,
        })
    };
    match runtime {
        RuntimeKind::Llama => match (primary, decision.is_grant()) {
            // Nothing in the primary: the launch takes it, unloading nothing.
            (None, _) => None,
            // Refused the second slot: the primary's resident is swapped out.
            (Some(resident), false) => unload(resident, UnloadReason::WouldNotFitBeside),
            // Granted the second slot: beside the primary; a model already in
            // that slot gives way first.
            (Some(_), true) => others
                .first()
                .and_then(|second| unload(second, UnloadReason::SecondSlotFull)),
        },
        RuntimeKind::StableDiffusion => {
            let image = primary
                .into_iter()
                .chain(others.iter())
                .find(|r| r.runtime == RuntimeKind::StableDiffusion);
            // The place an image model is judged against
            // (`too_large_for_its_place`): the loaded image model's slot
            // while another model is loaded too, else a primary that holds a
            // model; alone, it is placed whatever the budget.
            let place = match image {
                Some(loaded) if primary.is_some() && !others.is_empty() => Some(loaded),
                Some(_) => None,
                None => primary,
            };
            if place.is_some_and(|outgoing| !fits_in_place_of(decision, outgoing)) {
                return None;
            }
            if let Some(loaded) = image {
                return unload(loaded, UnloadReason::OneImageModel);
            }
            // No image model loaded (`may_use`): the second slot when granted
            // it or when the primary is empty, free or from its resident;
            // else the primary's resident.
            if decision.is_grant() || primary.is_none() {
                return others
                    .first()
                    .and_then(|second| unload(second, UnloadReason::SecondSlotFull));
            }
            primary.and_then(|resident| unload(resident, UnloadReason::WouldNotFitBeside))
        }
    }
}

/// Whether an image model with `decision` fits in the memory `outgoing`
/// gives back when swapped out, as placement judges it
/// (`QueueState::fits_in_place_of`): a grant fits; a verdict with a known
/// footprint and free reading is judged again with the outgoing weights
/// added back; one that knows no footprint or no free reading fits, which
/// places an unreadable budget as before.
fn fits_in_place_of(decision: &SecondarySlotDecision, outgoing: &Resident) -> bool {
    if decision.is_grant() {
        return true;
    }
    let (Some(needed), Some(free)) = (decision.footprint_bytes(), decision.free_bytes()) else {
        return true;
    };
    decide_secondary_slot_for(
        RuntimeKind::StableDiffusion,
        Some(SlotFootprint {
            weights_bytes: needed,
            kv_bytes: 0,
        }),
        Some(free.saturating_add(outgoing.weights_bytes)),
    )
    .is_grant()
}

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;
