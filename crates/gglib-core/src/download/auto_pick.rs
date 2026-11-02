//! Which variant of a repository a download takes when none is asked for:
//! the largest quantized one that fits this machine's memory.
//!
//! [`pick_variant`] walks the repository's weights variants largest first.
//! Each is judged from the first bytes of its first file, and only one that
//! fits has its type read, which can take a longer read: converters often
//! write `general.file_type` last, megabytes in. The first quantized variant
//! that fits is taken. When only `F32`, `F16` or `BF16` variants fit, the
//! largest of those is; when none fits, or there is no budget to judge by,
//! the smallest variant is.
//!
//! A file is a sidecar (a draft, an MTP head, an importance matrix) or a
//! projector only when its header shows it. A variant whose head shows one
//! is passed over, and one whose head left its role open is taken only once
//! its whole header has been read and shows weights, or could not be read.

use async_trait::async_trait;

use super::file_role::GgufFileRole;
use crate::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V};
use crate::domain::FileType;
use crate::domain::gguf::{GgufMetadata, HeadMetadata};
use crate::domain::kv_estimate::{
    estimate_kv_bytes_for_context, estimate_kv_elems_per_token, kv_bytes_per_token,
};
use crate::domain::phone_fit::{FitVerdict, MissingInput, fit_verdict};
use crate::domain::recommendation::usable_share;
use crate::ports::huggingface::HELPER_FILES_REFUSAL;
use crate::settings::DEFAULT_CONTEXT_SIZE;

/// One variant of a repository, as its listing gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contender {
    /// The size of every file of it, in bytes.
    pub size: u64,
    /// The role the listing's head of its first file settled; `None` when
    /// that head left it open.
    pub role: Option<GgufFileRole>,
}

/// What the first bytes of a variant's first file say.
#[derive(Debug, Clone, Default)]
pub struct HeadFacts {
    /// The metadata the head holds.
    pub head: HeadMetadata,
    /// Whether its tensor table names an image family. An image model's fit
    /// is its weights alone: it keeps no KV cache.
    pub image: bool,
}

/// Reads what [`pick_variant`] needs of a variant, by its index among the
/// contenders, when it needs it.
#[async_trait]
pub trait VariantReader: Send + Sync {
    /// The head of the variant's first file; `None` when it could not be
    /// read.
    async fn head(&self, index: usize) -> Option<HeadFacts>;

    /// The whole of the variant's first file's metadata, as far as its
    /// header goes, for its type and settled role; `None` when it could not
    /// be read.
    async fn header(&self, index: usize) -> Option<GgufMetadata>;
}

/// Whether a model fits, as far as what was read of it tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// It fits.
    Fits,
    /// It does not fit.
    DoesNotFit,
    /// A key the fit needs is missing from what was read: a longer read of
    /// the header may hold it.
    CannotTell,
}

/// Whether a model fits.
pub trait FitJudge: Send + Sync {
    /// Whether a model with `metadata`, and `weights_bytes` of weights, fits.
    /// `image` says it is an image model.
    fn fit(&self, metadata: &GgufMetadata, image: bool, weights_bytes: u64) -> Fit;

    /// Whether `weights_bytes` of weights fit alone: the fit of a model whose
    /// whole header leaves its trained context or KV shape unknown.
    fn weights_fit(&self, weights_bytes: u64) -> bool;
}

/// The fit a launch sizes its context by, against one machine's memory.
///
/// A chat model fits when the verdict is [`FitVerdict::Fits`]: a context
/// above the lowest rung. [`FitVerdict::Tight`] counts only when that rung is
/// the whole context the model was trained for. A model trained for fewer
/// tokens than the lowest rung, which the ladder never serves, fits when its
/// weights and the KV cache of its whole trained context do. Its trained
/// context and KV shape come from its metadata, as a phone's fit reads them
/// from a head, and a missing trained context or KV shape is
/// [`Fit::CannotTell`]. An image model fits when its weights fit the share of
/// the budget the fit spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryFit {
    /// The machine's memory, in bytes.
    pub budget_bytes: u64,
}

impl MemoryFit {
    /// Whether `bytes` fit the share of the budget a fit spends.
    fn holds(self, bytes: u64) -> Fit {
        if bytes <= usable_share(self.budget_bytes) {
            Fit::Fits
        } else {
            Fit::DoesNotFit
        }
    }
}

impl FitJudge for MemoryFit {
    fn fit(&self, metadata: &GgufMetadata, image: bool, weights_bytes: u64) -> Fit {
        if image {
            return self.holds(weights_bytes);
        }
        let trained = metadata.context_length;
        let kv = estimate_kv_elems_per_token(&metadata.metadata, metadata.architecture.as_deref());
        let (verdict, _) = fit_verdict(trained, Some(weights_bytes), kv, Some(self.budget_bytes));
        match (verdict, trained, kv) {
            (FitVerdict::Fits { .. }, ..) => Fit::Fits,
            // The lowest rung is all the model was trained for.
            (FitVerdict::Tight { ctx }, Some(trained), _) if ctx >= trained => Fit::Fits,
            // The ladder starts above the trained context: the whole of it.
            (FitVerdict::TooBig { .. }, Some(trained), Some(kv))
                if trained < DEFAULT_CONTEXT_SIZE =>
            {
                let per_token = kv_bytes_per_token(kv, DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V);
                self.holds(
                    weights_bytes.saturating_add(estimate_kv_bytes_for_context(per_token, trained)),
                )
            }
            (
                FitVerdict::Unknown {
                    missing: MissingInput::TrainedContext | MissingInput::KvShape,
                },
                ..,
            ) => Fit::CannotTell,
            _ => Fit::DoesNotFit,
        }
    }

    fn weights_fit(&self, weights_bytes: u64) -> bool {
        self.holds(weights_bytes) == Fit::Fits
    }
}

/// The variant [`pick_variant`] took, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    /// Its index among the contenders.
    pub index: usize,
    /// Why it was taken.
    pub reason: PickReason,
}

/// Why a variant was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickReason {
    /// The largest quantized variant that fits.
    Quantized(FileType),
    /// The largest variant that fits; none quantized does. `None` when its
    /// header states no type, or could not be read.
    LargestFitting(Option<FileType>),
    /// The smallest variant: none fits.
    NoneFits,
    /// The smallest variant: there was no budget to judge a fit by.
    NoBudget,
}

/// The repository holds no weights, only files that run beside a model:
/// drafts, MTP heads, importance matrices, projectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoWeights;

impl std::fmt::Display for NoWeights {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "only draft or helper files: {HELPER_FILES_REFUSAL}")
    }
}

impl std::error::Error for NoWeights {}

/// The variant a download takes among `contenders` when none is asked for.
///
/// Reads nothing ahead of need: a head per variant walked, and a whole
/// header only for a variant whose fit its head cannot tell, or that fits
/// and whose head left its type or role open. A variant whose whole header
/// leaves its trained context or KV shape unknown is judged by its weights
/// alone. With no `judge` (no budget), the
/// smallest variant is taken and no fit is judged.
///
/// # Errors
///
/// [`NoWeights`] when no contender is weights, or every one turns out not to
/// be.
pub async fn pick_variant(
    contenders: &[Contender],
    reader: &dyn VariantReader,
    judge: Option<&dyn FitJudge>,
) -> Result<Pick, NoWeights> {
    let mut largest_first: Vec<usize> = (0..contenders.len())
        .filter(|&index| may_be_weights(contenders[index].role))
        .collect();
    largest_first.sort_by_key(|&index| std::cmp::Reverse(contenders[index].size));
    if largest_first.is_empty() {
        return Err(NoWeights);
    }
    let Some(judge) = judge else {
        return smallest(contenders, &largest_first, reader, PickReason::NoBudget).await;
    };

    let mut largest_unquantized = None;
    // The variants still in the running for the smallest: none the walk
    // found is not weights.
    let mut weights = largest_first.clone();
    for &index in &largest_first {
        let size = contenders[index].size;
        let Some(head) = reader.head(index).await else {
            continue;
        };
        if !may_be_weights(head.head.role) {
            weights.retain(|&other| other != index);
            continue;
        }
        let mut header = None;
        let fits = match judge.fit(&head.head.metadata, head.image, size) {
            Fit::Fits => true,
            Fit::DoesNotFit => false,
            // The whole header leaves the shape unknown: the weights alone.
            Fit::CannotTell if head.head.complete => judge.weights_fit(size),
            Fit::CannotTell => {
                header = reader.header(index).await;
                match header
                    .as_ref()
                    .map(|whole| judge.fit(whole, head.image, size))
                {
                    Some(Fit::Fits) => true,
                    Some(Fit::CannotTell) => judge.weights_fit(size),
                    Some(Fit::DoesNotFit) | None => false,
                }
            }
        };
        if !fits {
            continue;
        }
        let whole = if head.head.complete {
            Some(head.head.metadata)
        } else if header.is_some() {
            header
        } else {
            reader.header(index).await
        };
        let (role, file_type) =
            whole.map_or((None, None), |whole| (Some(whole.role), whole.file_type));
        if !may_be_weights(role) {
            weights.retain(|&other| other != index);
            continue;
        }
        match file_type {
            Some(file_type) if file_type.is_quantized() => {
                return Ok(Pick {
                    index,
                    reason: PickReason::Quantized(file_type),
                });
            }
            other => {
                largest_unquantized.get_or_insert(Pick {
                    index,
                    reason: PickReason::LargestFitting(other),
                });
            }
        }
    }
    match largest_unquantized {
        Some(pick) => Ok(pick),
        None => smallest(contenders, &weights, reader, PickReason::NoneFits).await,
    }
}

/// Whether a file of `role` may be weights: a settled weights role, or one
/// left open.
fn may_be_weights(role: Option<GgufFileRole>) -> bool {
    matches!(role, None | Some(GgufFileRole::Weights))
}

/// The smallest of `largest_first` whose header does not show it is not
/// weights. A head or header that cannot be read shows nothing.
async fn smallest(
    contenders: &[Contender],
    largest_first: &[usize],
    reader: &dyn VariantReader,
    reason: PickReason,
) -> Result<Pick, NoWeights> {
    for &index in largest_first.iter().rev() {
        if contenders[index].role != Some(GgufFileRole::Weights)
            && !shows_weights(reader, index).await
        {
            continue;
        }
        return Ok(Pick { index, reason });
    }
    Err(NoWeights)
}

/// Whether the variant at `index`, whose listing left its role open, is
/// weights as far as its header shows.
async fn shows_weights(reader: &dyn VariantReader, index: usize) -> bool {
    match reader.head(index).await {
        Some(head) if head.head.role.is_some() => may_be_weights(head.head.role),
        _ => reader
            .header(index)
            .await
            .is_none_or(|whole| whole.role == GgufFileRole::Weights),
    }
}

#[cfg(test)]
#[path = "auto_pick_tests.rs"]
mod tests;
