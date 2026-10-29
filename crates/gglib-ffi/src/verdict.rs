//! Whether a model fits the device, as the app is told it.
//!
//! The verdict is gglib-core's (`domain::phone_fit::fit_verdict`), reached
//! from a model's shape (its trained context and KV cache per token), the
//! size of its weights and the device's budget. This module only reads the
//! shape off what there is to read, a header's first bytes, an installed
//! model's row or a shortlist row, and carries the verdict across.

use gglib_core::domain::phone_fit::{self, fit_verdict};
use gglib_core::domain::recommendation::ModelCandidate;
use gglib_core::domain::{FitInputs, KvElemsPerToken, Model, estimate_kv_elems_per_token};
use gglib_core::ports::HeadMetadata;

/// Whether a model fits the device, in the words a person reads it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FitVerdict {
    /// A context above the smallest gglib serves fits; `ctx` is the largest
    /// that does, in tokens.
    Fits {
        /// The context the budget serves.
        ctx: u64,
    },
    /// Only the smallest context gglib serves fits; `ctx` is that size.
    Tight {
        /// The context the budget serves.
        ctx: u64,
    },
    /// Not even the smallest context fits: the model needs a budget of
    /// `need_bytes` and the device gives `have_bytes`. A model trained for
    /// fewer tokens than the smallest context is too big however much
    /// memory there is, and then `need_bytes` can be below `have_bytes`
    /// ([`FitWhy::trained_ctx`] says so).
    TooBig {
        /// The smallest budget the smallest context fits in, in bytes.
        need_bytes: u64,
        /// The budget the device gives, in bytes.
        have_bytes: u64,
    },
    /// gglib could not tell without `missing`.
    Unknown {
        /// The input that is missing: the first of trained context, KV
        /// shape, weights and budget.
        missing: MissingInput,
        /// The input was to be read from the first bytes of the model's
        /// file, and those ended before its header did: the file may still
        /// hold it, and a download can tell. `false` when the file, or
        /// whatever else the verdict was read from, does not hold it.
        cut_off: bool,
    },
}

/// What a [`FitVerdict::Unknown`] could not do without.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MissingInput {
    /// The model's trained context: `{arch}.context_length` in its GGUF.
    TrainedContext,
    /// The model's KV shape: its layers, KV heads and head dimensions.
    KvShape,
    /// The size of the weights.
    Weights,
    /// The memory budget, from a reading of the device.
    Budget,
}

/// The numbers a [`FitVerdict`] was reached from, for a sheet that says
/// why. A number that is absent was unknown, never zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FitWhy {
    /// The weights, every shard of them, in bytes.
    pub weights_bytes: Option<u64>,
    /// What each token of context costs in KV cache, in bytes, at the cache
    /// types gglib serves with.
    pub kv_bytes_per_token: Option<u64>,
    /// The budget the device gives, in bytes.
    pub budget_bytes: Option<u64>,
    /// The context the model was trained for, in tokens. A model trained for
    /// fewer than the smallest context gglib serves (4096 tokens) is
    /// [`FitVerdict::TooBig`] at any memory size: this is the number to word
    /// that case with.
    pub trained_ctx: Option<u64>,
}

impl From<FitInputs> for FitWhy {
    fn from(inputs: FitInputs) -> Self {
        Self {
            weights_bytes: inputs.weights_bytes,
            kv_bytes_per_token: inputs.kv_bytes_per_token,
            budget_bytes: inputs.budget_bytes,
            trained_ctx: inputs.trained_ctx,
        }
    }
}

/// What a fit needs to know of a model besides its weights: its trained
/// context and KV shape, and whether they were read from a header cut short.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModelShape {
    trained_ctx: Option<u64>,
    kv: Option<KvElemsPerToken>,
    /// What was read is a head that ended before the header did.
    cut_off: bool,
}

impl ModelShape {
    /// The shape the first bytes of a model's file give: the keys a fit
    /// needs that the head holds. One it does not hold is cut off when the
    /// head ended before the header ([`HeadMetadata::complete`]), and
    /// missing from the file otherwise.
    pub(crate) fn of_head(head: &HeadMetadata) -> Self {
        let metadata = &head.metadata;
        Self {
            trained_ctx: metadata.context_length,
            kv: estimate_kv_elems_per_token(&metadata.metadata, metadata.architecture.as_deref()),
            cut_off: !head.complete,
        }
    }

    /// The shape an installed model's row records, read from its whole
    /// header at import.
    pub(crate) fn of_model(model: &Model) -> Self {
        Self {
            trained_ctx: model.context_length,
            kv: estimate_kv_elems_per_token(&model.metadata, model.architecture.as_deref()),
            cut_off: false,
        }
    }

    /// The shape a shortlist row states.
    pub(crate) const fn of_candidate(candidate: &ModelCandidate) -> Self {
        Self {
            trained_ctx: Some(candidate.trained_ctx),
            kv: Some(candidate.kv_elems_per_token),
            cut_off: false,
        }
    }

    /// How this model, with `weights_bytes` of weights, fits `budget_bytes`.
    pub(crate) fn judge(
        &self,
        weights_bytes: Option<u64>,
        budget_bytes: Option<u64>,
    ) -> (FitVerdict, FitWhy) {
        let (verdict, inputs) = fit_verdict(self.trained_ctx, weights_bytes, self.kv, budget_bytes);
        (carried(verdict, self.cut_off), inputs.into())
    }
}

/// gglib-core's verdict in this library's terms, for a model whose shape was
/// read from a head that ended before the header did when `cut_off`.
pub(crate) fn carried(verdict: phone_fit::FitVerdict, cut_off: bool) -> FitVerdict {
    match verdict {
        phone_fit::FitVerdict::Fits { ctx } => FitVerdict::Fits { ctx },
        phone_fit::FitVerdict::Tight { ctx } => FitVerdict::Tight { ctx },
        phone_fit::FitVerdict::TooBig {
            need_bytes,
            have_bytes,
        } => FitVerdict::TooBig {
            need_bytes,
            have_bytes,
        },
        phone_fit::FitVerdict::Unknown { missing } => {
            // Only the shape is read from the header; the weights and the
            // budget never are.
            let (missing, from_header) = match missing {
                phone_fit::MissingInput::TrainedContext => (MissingInput::TrainedContext, true),
                phone_fit::MissingInput::KvShape => (MissingInput::KvShape, true),
                phone_fit::MissingInput::Weights => (MissingInput::Weights, false),
                phone_fit::MissingInput::Budget => (MissingInput::Budget, false),
            };
            FitVerdict::Unknown {
                missing,
                cut_off: from_header && cut_off,
            }
        }
    }
}

#[cfg(test)]
#[path = "verdict_tests.rs"]
mod tests;
