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

use gglib_core::cache_config::KvCacheType;
use gglib_core::domain::{FitInputs, KvElemsPerToken, fit_context_explained};

use super::{fit_or_undivided, vram};

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

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;
