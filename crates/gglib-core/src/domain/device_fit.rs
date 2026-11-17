//! The context a load on this machine fits a model to: against the budget
//! less a fixed room for a second resident model, and when nothing fits
//! there, against the whole budget ([`fit_on_device`]).
//!
//! The one rule a launch, a plan of one (`gglib model explain`, the
//! inspector), the size picker and the first-model suggestion all answer
//! "does this fit, and at what context?" by. The picker and the suggestion
//! give it what a load of the size counts (its weights and the projector a
//! download fetches with them) at the KV cache types a launch uses when
//! none is asked for, so a size the picker calls fitting is one a load
//! fits, at the context it names; a launch told other KV types, or a
//! `default-context-size`, is served otherwise, and the picker then names
//! no context (`MemoryFit`). A phone has one
//! slot and no room to keep: its load fits by
//! [`fit_verdict`](super::phone_fit::fit_verdict) alone.

use crate::cache_config::KvCacheType;
use crate::domain::context_fit::{FitInputs, fit_context_explained};
use crate::domain::kv_estimate::KvShape;
use crate::domain::phone_fit::{FitVerdict, verdict};
use crate::domain::recommendation::BUDGET_UTILISATION;
use crate::domain::residency::{RESIDENCY_UTILISATION, SECONDARY_MAX_BYTES};

/// The memory a fitted context may be sized against on a device of `total`
/// bytes.
///
/// The device less a fixed reservation for the second resident slot, the
/// ceiling [`decide_secondary_slot`](super::decide_secondary_slot) holds a
/// co-resident to ([`SECONDARY_MAX_BYTES`]).
///
/// A *fixed* reservation rather than netting out whoever is resident right
/// now, because the fitted context is part of a resident's identity: a request
/// resolving to a different one evicts and relaunches. Budgeting against the
/// live resident set makes the answer move when a co-resident loads or is
/// evicted, and each move costs the primary a full teardown, weight reload and
/// prompt re-prefill — the exact cost the second slot exists to avoid. It also
/// made the budget depend on whether some *other* model's KV shape was
/// readable, so one unreadable GGUF denied every other model a fit.
///
/// The reservation is a *top-up*, not a flat subtraction. A flat subtraction
/// was measured across five model shapes and eight devices: of the
/// twenty-six configurations that produced a fit, it changed the co-load
/// verdict in one, and in four others cost the primary between one and four
/// rungs for a secondary that either fitted anyway or could not fit
/// regardless. Taking only what the utilisation margin does not already
/// leave keeps the co-load in the case that needed it and gives the rungs
/// back everywhere else.
///
/// It remains a per-process constant, so the budget still provably never
/// moves. [`fit_on_device`] falls back to the undivided device when
/// reserving would mean no fit at all.
#[must_use]
pub fn fit_budget_from(total: u64) -> u64 {
    // Only top up what the utilisation margin does not already leave free.
    //
    // `fit_context` spends at most `BUDGET_UTILISATION` of what it is given, so
    // a primary fitted against `total - R` occupies about that fraction of it
    // and leaves roughly `(1 - BUDGET_UTILISATION) * total` free before `R` is
    // counted. On a 24 GiB card that is 2.4 GiB with `R = 0`, which already
    // covers a full-ceiling secondary; subtracting a flat 2 GiB on top
    // double-counts and costs the primary a rung for nothing.
    //
    // The target is not the ceiling itself. `decide_secondary_slot` admits a
    // candidate only when it fits *and* clears `RESIDENCY_UTILISATION` of the
    // live free reading, so admitting a full-ceiling secondary needs the
    // ceiling divided by that fraction — about 2.22 GiB, not 2.
    let target_free = scale_up(SECONDARY_MAX_BYTES, RESIDENCY_UTILISATION);
    let already_free = scale(total, 1.0 - BUDGET_UTILISATION);

    // Withholding `R` yields only `BUDGET_UTILISATION * R` of extra free, since
    // the primary spends that fraction of whatever it is given — so the
    // shortfall is grossed up rather than subtracted raw.
    let withhold = scale_up(target_free.saturating_sub(already_free), BUDGET_UTILISATION);
    total.saturating_sub(withhold)
}

/// `bytes * factor`, saturating and rounding down.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "a byte count scaled by a fraction, rounded down"
)]
fn scale(bytes: u64, factor: f64) -> u64 {
    (bytes as f64 * factor) as u64
}

/// `bytes / factor` — what you must have so that `factor` of it is `bytes`.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "a byte count grossed up by a fraction, rounded down"
)]
fn scale_up(bytes: u64, factor: f64) -> u64 {
    if factor <= 0.0 {
        return u64::MAX;
    }
    (bytes as f64 / factor) as u64
}

/// Fit against the reserved budget, falling back to the undivided device.
///
/// A seam, not indirection: the chain is the whole of the co-resident
/// reservation's escape hatch, and inlining it left the behaviour unguarded —
/// deleting the fallback passed every test in the crate.
pub fn fit_or_undivided<F>(fit: F, reserved: Option<u64>, undivided: Option<u64>) -> Option<u64>
where
    F: Fn(Option<u64>) -> Option<u64>,
{
    fit(reserved).or_else(|| fit(undivided))
}

/// The context a load fits a model to inside `budget_bytes`, and what that
/// answer worked from.
///
/// `budget_bytes` is the machine's memory as the load reads it. The reserved
/// budget ([`fit_budget_from`]) is tried first, then the undivided one
/// ([`fit_or_undivided`]). The [`FitInputs`] are those of the budget that answered, the last one
/// tried. Its `system_ram_bytes` is left for the caller that chose the
/// budget to set. Known consequence, deliberate: the served context is not
/// monotonic in device size, nor in the size of the weights: a size that
/// fits the reserved budget is served there, and a larger one that does not
/// falls back to the whole device.
#[must_use]
pub fn fit_on_device(
    trained_ctx: Option<u64>,
    weights_bytes: Option<u64>,
    kv: Option<KvShape>,
    k: KvCacheType,
    v: KvCacheType,
    budget_bytes: Option<u64>,
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
        budget_bytes.map(fit_budget_from),
        budget_bytes,
    );
    let inputs = captured
        .into_inner()
        .expect("fit_or_undivided calls the closure at least once");
    (fitted, inputs)
}

/// [`fit_on_device`] at the KV cache types `k` and `v`, as a
/// [`FitVerdict`]: the words the size picker says beside each size, from
/// the context the load would serve it at.
#[must_use]
pub fn device_fit_verdict(
    trained_ctx: Option<u64>,
    weights_bytes: Option<u64>,
    kv: Option<KvShape>,
    k: KvCacheType,
    v: KvCacheType,
    budget_bytes: Option<u64>,
) -> (FitVerdict, FitInputs) {
    let (fitted, inputs) = fit_on_device(trained_ctx, weights_bytes, kv, k, v, budget_bytes);
    (verdict(fitted, &inputs), inputs)
}

#[cfg(test)]
#[path = "device_fit_tests.rs"]
mod tests;
