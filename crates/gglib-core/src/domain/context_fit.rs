//! How large a context this machine can actually serve.
//!
//! `gglib up` already does this arithmetic: it picks a model by asking what
//! fits in VRAM at 32k and prints the answer as the number that earns the
//! user's trust. The launch path then resolved its context from a chain whose
//! lowest reachable rung was a flat 4096 and served that instead — so the
//! number shown and the number used were unrelated.
//!
//! This is the launch-time half of that arithmetic. `up`'s shortlist still
//! asks a different question — "does this model fit *at* 32k?" — so the two
//! are not one calculation and cannot be made one cheaply: they are answered
//! against different budgets, and the shortlist runs before the model is
//! downloaded, when its real KV geometry is not yet readable.
//!
//! What changed is that the banner no longer implies otherwise. It reports 32k
//! as the bar the model had to clear, and says the served context is sized at
//! launch, which is true and knowable. Naming a rung there would have been the
//! same error in the other direction.
//!
//! ## Why it snaps to rungs
//!
//! A resident is identified partly by the context it was launched with, and a
//! request that resolves to a different one evicts and relaunches. A value
//! computed from a live free-memory reading would wobble between requests and
//! recycle the server — blowing the prefix cache and every saved slot file —
//! on essentially every turn. Snapping to a fixed ladder makes the result a
//! step function that changes only when the machine genuinely changes.

use crate::cache_config::KvCacheType;
use crate::domain::kv_estimate::{KvElemsPerToken, kv_bytes_per_token};
use crate::domain::recommendation::BUDGET_UTILISATION;

use crate::settings::DEFAULT_CONTEXT_SIZE;

/// `bytes * factor`, saturating and rounding down.
#[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_truncation)]
fn scale(bytes: u64, factor: f64) -> u64 {
    (bytes as f64 * factor) as u64
}

/// The context sizes a fitted value is allowed to take.
///
/// Powers of two from the built-in default upward. Deliberately coarse: the
/// point is stability, not squeezing out the last few thousand tokens.
const RUNGS: [u64; 6] = [4096, 8192, 16_384, 32_768, 65_536, 131_072];

// The ladder starts at the built-in default, so a fitted value can never be
// worse than serving no fit at all. Compile-time because both operands are
// constants and the mistake would be a source edit.
const _: () = assert!(RUNGS[0] == DEFAULT_CONTEXT_SIZE);

/// The largest context `weights_bytes` can serve inside `budget_bytes`.
///
/// `None` whenever the answer cannot be computed from facts — an unknown
/// trained context, unknown KV shape, or no memory reading. That is a refusal
/// rather than an optimistic guess, matching `SlotFootprint::new`: a caller
/// that gets `None` falls back down its own chain instead of launching against
/// a number nobody stands behind.
///
/// Also `None` when the machine cannot fit even the smallest rung. Returning
/// something smaller would be inventing a context this module has no basis
/// for; the built-in default is the right thing to fall back to, and it will
/// fail honestly if it does not fit either.
///
/// Pure. `GGLIB_DISABLE_CONTEXT_FIT` is read by the caller, not here — the
/// switch belongs with the other runtime switches at the admission site, and a
/// domain function that read the environment could not be tested without
/// mutating process-global state.
///
/// `budget_bytes` must be a figure that does not move between requests: this
/// value ends up in a resident's identity, so a budget that drifts evicts and
/// relaunches the model it just sized.
///
/// A live free-memory reading is therefore wrong — on Apple it is a fraction
/// of available system RAM, and it moves with whatever else is open. So is
/// netting out the current resident set, which was tried and removed: it moved
/// whenever a co-resident loaded or was evicted, and made one model's budget
/// depend on whether another model's KV shape was readable. What the caller
/// supplies is total device capacity less a fixed reservation for the second
/// resident slot.
#[must_use]
pub fn fit_context(
    trained_ctx: Option<u64>,
    weights_bytes: Option<u64>,
    kv: Option<KvElemsPerToken>,
    k: KvCacheType,
    v: KvCacheType,
    budget_bytes: Option<u64>,
) -> Option<u64> {
    fit_context_explained(trained_ctx, weights_bytes, kv, k, v, budget_bytes).0
}

/// What [`fit_context`] worked from, for a person reading a launch log.
///
/// The two constants governing this — `BUDGET_UTILISATION` and the caller's
/// co-resident reservation — are judgement calls, not measurements. Nothing
/// acts on this record; it exists so the numbers behind a fitted context are
/// visible when someone asks whether those judgements were right, rather than
/// having to be re-derived from the rung alone.
///
/// `gglib model explain` is where a person reads it, through
/// `residency::explain::explain_fit`, which supplies the same two budgets
/// `admit` does. Before that it reached only a `debug!` line written after a
/// launch, which is not a reading anyone could take across a catalog — and
/// ADR 0009's first kill criterion needs exactly that.
#[derive(Debug, Clone, Copy)]
pub struct FitInputs {
    /// Device memory the fit was allowed to spend against.
    pub budget_bytes: Option<u64>,
    /// The model's weights, as summed across shards.
    pub weights_bytes: Option<u64>,
    /// Bytes of KV cache each token of context costs at the resolved types.
    pub kv_bytes_per_token: Option<u64>,
    /// The model's trained ceiling.
    pub trained_ctx: Option<u64>,
    /// The context that fit before snapping to a rung — the difference between
    /// this and the chosen rung is what the ladder costs.
    pub unsnapped: Option<u64>,
}

/// The same calculation, reporting what it worked from.
#[must_use]
pub fn fit_context_explained(
    trained_ctx: Option<u64>,
    weights_bytes: Option<u64>,
    kv: Option<KvElemsPerToken>,
    k: KvCacheType,
    v: KvCacheType,
    budget_bytes: Option<u64>,
) -> (Option<u64>, FitInputs) {
    let per_token = kv
        .map(|elems| kv_bytes_per_token(elems, k, v))
        .filter(|&b| b > 0);
    // `0` is this codebase's sentinel for "size could not be read"
    // (`total_model_bytes`), and it is the most dangerous value to take
    // literally: weights that cost nothing hand the entire budget to the KV
    // cache. `launch_deadline_secs` reads the same field and refuses it the
    // same way.
    let weights = weights_bytes.filter(|&b| b > 0);
    // Never claim more than the model was trained for, *before* snapping, so
    // the result is always both a rung and within the model's range.
    let unsnapped = (|| {
        let usable = scale(budget_bytes?, BUDGET_UTILISATION);
        let for_kv = usable.checked_sub(weights?)?;
        Some((for_kv / per_token?).min(trained_ctx?))
    })();

    let inputs = FitInputs {
        budget_bytes,
        weights_bytes: weights,
        kv_bytes_per_token: per_token,
        trained_ctx,
        unsnapped,
    };
    let fitted = unsnapped.and_then(|u| RUNGS.iter().rev().find(|&&rung| rung <= u).copied());
    (fitted, inputs)
}

#[cfg(test)]
#[path = "context_fit_tests.rs"]
mod tests;
