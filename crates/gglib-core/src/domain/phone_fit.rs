//! Whether a model fits a phone, as a verdict with the numbers behind it.
//!
//! A person choosing a model to download to a phone needs a word, not a
//! context size: whether this phone can run it. [`fit_verdict`] gives that
//! word, and hands back the [`FitInputs`] it was reached from so a "why" can be
//! shown beside it.
//!
//! It is not a second fit. The verdict is read off [`fit_context_explained`],
//! the arithmetic a launch sizes its context with, at the KV cache types a
//! launch uses unless told otherwise ([`DEFAULT_CACHE_TYPE_K`],
//! [`DEFAULT_CACHE_TYPE_V`]), so a verdict and the context the same budget
//! would serve at those types cannot disagree. It keeps that module's rule of
//! refusing rather than guessing: a missing input is [`FitVerdict::Unknown`],
//! naming the input, and never an optimistic "fits".

use crate::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V};
use crate::domain::context_fit::{FitInputs, fit_context_explained};
use crate::domain::kv_estimate::{KvElemsPerToken, estimate_kv_bytes_for_context};
use crate::domain::recommendation::budget_covering;
use crate::settings::DEFAULT_CONTEXT_SIZE;

/// The lowest rung of the context ladder: `context_fit` asserts at compile
/// time that its ladder starts at the built-in default.
const LOWEST_RUNG: u64 = DEFAULT_CONTEXT_SIZE;

/// Whether a model fits, in the words a person reads it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitVerdict {
    /// A rung above the lowest fits; `ctx` is the largest that does.
    Fits {
        /// The context the budget serves, a rung of the ladder.
        ctx: u64,
    },
    /// Only the lowest rung fits; `ctx` is that rung.
    ///
    /// Memory, or a model trained for fewer tokens than the next rung, holds
    /// it there; [`FitInputs`] says which.
    Tight {
        /// The context the budget serves, always the ladder's lowest rung.
        ctx: u64,
    },
    /// Every input is known and not even the lowest rung fits.
    ///
    /// `need_bytes` is the smallest budget the lowest rung fits in: the
    /// weights plus that rung's KV cache at the default KV types, grown by the
    /// share of a budget the fit holds back ([`budget_covering`]).
    /// `have_bytes` is the budget the verdict was given. Both are budgets, so
    /// they compare directly, and when memory is the reason `need_bytes` is
    /// greater than `have_bytes`. [`FitInputs`] holds the weights and the KV
    /// cost per token it was built from.
    ///
    /// A model trained for fewer tokens than the lowest rung is too big as
    /// well, however much memory there is: the fit never invents a context
    /// below the ladder, so it serves such a model none, and a verdict offers
    /// nothing the fit refuses. Then `need_bytes` can sit below `have_bytes`,
    /// and the reason is the `trained_ctx` under the lowest rung that its
    /// [`FitInputs`] show.
    TooBig {
        /// The smallest budget the lowest rung fits in, in bytes.
        need_bytes: u64,
        /// The budget the fit was given, in bytes.
        have_bytes: u64,
    },
    /// An input the fit needs is missing.
    Unknown {
        /// The input that is missing; when several are, the first of trained
        /// context, KV shape, weights and budget, in that order.
        missing: MissingInput,
    },
}

/// The input a [`FitVerdict::Unknown`] could not do without.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingInput {
    /// The model's trained context, `{arch}.context_length` in its GGUF.
    TrainedContext,
    /// The model's KV shape: layers, KV heads and head dimensions.
    KvShape,
    /// The size of the weights. Zero counts as missing, as it does in the fit.
    Weights,
    /// The memory budget, from a reading of the device.
    Budget,
}

/// How a model with these facts fits inside `budget_bytes`, and what that
/// verdict was reached from.
///
/// Pure. `budget_bytes` should be one reading, held for as long as the
/// verdicts it produced are on screen, for the reason `context_fit` gives: a
/// figure that moves between calls moves the answers with it.
#[must_use]
pub fn fit_verdict(
    trained_ctx: Option<u64>,
    weights_bytes: Option<u64>,
    kv: Option<KvElemsPerToken>,
    budget_bytes: Option<u64>,
) -> (FitVerdict, FitInputs) {
    let (fitted, inputs) = fit_context_explained(
        trained_ctx,
        weights_bytes,
        kv,
        DEFAULT_CACHE_TYPE_K,
        DEFAULT_CACHE_TYPE_V,
        budget_bytes,
    );
    (verdict(fitted, &inputs), inputs)
}

/// The verdict for a fitted rung, or for the inputs that produced none.
///
/// Read from [`FitInputs`] rather than the arguments, because the fit treats a
/// zero weights figure or a zero-cost KV shape as missing and the verdict must
/// agree with it.
fn verdict(fitted: Option<u64>, inputs: &FitInputs) -> FitVerdict {
    if let Some(ctx) = fitted {
        return if ctx > LOWEST_RUNG {
            FitVerdict::Fits { ctx }
        } else {
            FitVerdict::Tight { ctx }
        };
    }
    match (
        inputs.trained_ctx,
        inputs.kv_bytes_per_token,
        inputs.weights_bytes,
        inputs.budget_bytes,
    ) {
        (None, ..) => FitVerdict::Unknown {
            missing: MissingInput::TrainedContext,
        },
        (_, None, ..) => FitVerdict::Unknown {
            missing: MissingInput::KvShape,
        },
        (_, _, None, _) => FitVerdict::Unknown {
            missing: MissingInput::Weights,
        },
        (_, _, _, None) => FitVerdict::Unknown {
            missing: MissingInput::Budget,
        },
        (Some(_), Some(per_token), Some(weights), Some(budget)) => FitVerdict::TooBig {
            need_bytes: budget_covering(
                weights.saturating_add(estimate_kv_bytes_for_context(per_token, LOWEST_RUNG)),
            ),
            have_bytes: budget,
        },
    }
}

#[cfg(test)]
#[path = "phone_fit_tests.rs"]
mod tests;
