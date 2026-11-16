//! The fit this machine would compute, without launching anything.
//!
//! `gglib model explain` answers "what would happen", and ADR 0009's first kill
//! criterion needs the same answer across a catalog: if the chosen rung is
//! routinely far below the unsnapped figure, the ladder is too coarse. A
//! launch records the same fit on its narration.
//!
//! This is the same call `admit` makes — the same reserved budget, the same
//! fallback to the undivided device — so an explanation cannot describe a fit
//! that differs from the one a launch performs. It is deliberately *not* a
//! second implementation: `fit_or_undivided` is the shared seam, and this
//! module supplies the same two budgets to it.

use std::path::Path;

use gglib_core::cache_config::{CacheRamSetting, KvCacheType};
use gglib_core::domain::{
    FitInputs, KvShape, LaunchPlan, RuntimeKind, SecondarySlotDecision, SlotFootprint, Unload,
    UnloadReason, decide_secondary_slot_for, fit_context_explained,
};
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec, ModelRuntimeError};
use gglib_core::server_config::ServerConfigOptions;
use gglib_core::utils::process::bounded::Failure;

use super::context::fit_or_undivided;
use super::launch::{LaunchRequest, LlamaResolutions, plan_llama, plan_sd};
use super::{explain_args, prepare, vram};
use crate::engine::NotWorking;
use crate::process::admission::Resident;
use crate::process::{RuntimeBinaries, SpawnConfig};
use crate::system::FitMemory;

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
    explain_fit_against(
        crate::system::fit_memory(),
        trained_ctx,
        weights_bytes,
        kv,
        k,
        v,
    )
}

/// [`explain_fit`] against `memory`, the machine's reading lifted into a
/// parameter so every shape of machine can be asserted on any one.
#[must_use]
pub(super) fn explain_fit_against(
    memory: FitMemory,
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
    let undivided = memory.budget_bytes();
    let fitted = fit_or_undivided(
        |budget| {
            let (rung, inputs) =
                fit_context_explained(trained_ctx, weights_bytes, kv, k, v, budget);
            captured.set(Some(inputs));
            rung
        },
        undivided.map(vram::fit_budget_from),
        undivided,
    );
    let mut inputs = captured
        .into_inner()
        .expect("fit_or_undivided calls the closure at least once");
    inputs.system_ram_bytes = memory.system_ram_bytes();
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
/// program is the engine of `binaries` the launch would run, by its path
/// (`engine`). The arguments are the spawn's own token list with the port
/// spelled `0`, which the engine reads as any free port, each with the
/// decision behind it (`explain_args`); the fit is what the context was
/// sized by. Planned as a launch into the primary slot: the queue chooses
/// the slot when the launch happens, and a second slot's cache differs.
///
/// Only the engine is looked for on disk, so the plan can say it is not
/// installed: any other file that is missing is the inspector's to name,
/// and a plan of a model whose weights are gone is still the plan.
///
/// # Errors
///
/// As a launch's planning: an image model with no family.
#[expect(
    clippy::too_many_arguments,
    reason = "what a launch is planned from, each its own value: the model, its engine, the cascade and what is loaded"
)]
pub fn plan_launch(
    spec: ModelLaunchSpec,
    binaries: &RuntimeBinaries,
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
    let request = prepare(spec, template, cache_ram, num_ctx, default_ctx, overrides);
    let planned = match request.spec.runtime() {
        RuntimeKind::Llama => plan_llama(others, &request),
        RuntimeKind::StableDiffusion => plan_sd(&request)?,
    };
    let args = match (&planned.config, &planned.llama) {
        (SpawnConfig::Llama(config), Some(resolved)) => {
            explain_args::llama_args(config, resolved, &request)
        }
        (SpawnConfig::Sd(config), _) => explain_args::sd_args(config),
        (SpawnConfig::Llama(_), None) => {
            return Err(ModelRuntimeError::Internal(
                "a llama-server plan carried no resolutions".to_owned(),
            ));
        }
    };
    let decision = second_slot_decision(&request, planned.llama.as_ref());
    let unloads = displacement(
        request.spec.runtime(),
        &decision,
        primary,
        others,
        already_resident,
    );
    let (program, not_installed) = engine(
        request.spec.runtime(),
        binaries.for_runtime(request.spec.runtime()),
    );
    Ok(LaunchPlan {
        program,
        args,
        not_installed,
        narration: planned.narration,
        unloads,
        // The same value the narration carries: one fit, read twice.
        fit: request.fit,
    })
}

/// The engine a launch of `runtime` runs, by its path, from `binary`, the
/// one its process core holds: for llama-server the spawn's own choice
/// ([`llama_binary`](crate::command::llama_binary)), for `sd-server`
/// `binary` itself. When none is there, `binary` (where gglib installs it)
/// and the sentence a load that tried would fail with
/// ([`NotWorking::when_started`]): it is not found, and the command that
/// installs it.
fn engine(runtime: RuntimeKind, binary: &Path) -> (String, Option<String>) {
    let found = match runtime {
        RuntimeKind::Llama => crate::command::llama_binary(Some(binary)).ok(),
        RuntimeKind::StableDiffusion => binary.is_file().then(|| binary.to_path_buf()),
    };
    found.map_or_else(
        || {
            (
                binary.to_string_lossy().into_owned(),
                Some(NotWorking::when_started(runtime, binary, Failure::NotFound).to_string()),
            )
        },
        |path| (path.to_string_lossy().into_owned(), None),
    )
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
    let unload =
        |r: &Resident, reason: UnloadReason| Some(Unload::new(r.model_name.clone(), reason));
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
