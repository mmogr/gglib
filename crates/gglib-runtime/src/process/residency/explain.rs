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
    ContextFit, FitInputs, KvElemsPerToken, LaunchPlan, RuntimeKind, fit_context_explained,
};
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec, ModelRuntimeError};
use gglib_core::server_config::ServerConfigOptions;

use super::context::fit_or_undivided;
use super::launch::{plan_llama, plan_sd};
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
    kv: Option<KvElemsPerToken>,
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
    others: &[Resident],
) -> Result<LaunchPlan, ModelRuntimeError> {
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
    Ok(LaunchPlan {
        program: program.to_owned(),
        args,
        narration: planned.narration,
        fit: fit.map(|(fitted, inputs)| ContextFit {
            serves_at,
            source,
            fitted,
            budget_bytes: inputs.budget_bytes,
            weights_bytes: inputs.weights_bytes,
            kv_bytes_per_token: inputs.kv_bytes_per_token,
            trained_ctx: inputs.trained_ctx,
            unsnapped: inputs.unsnapped,
        }),
    })
}

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;
