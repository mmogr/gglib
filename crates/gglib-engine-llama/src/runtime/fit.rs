//! The context a model is loaded with on this device, or why it is not.

use gglib_core::DEFAULT_CONTEXT_SIZE;
use gglib_core::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V, KvCacheType};
use gglib_core::domain::phone_fit::{FitVerdict, MissingInput, fit_verdict};
use gglib_core::ports::{ModelLaunchSpec, ModelRuntimeError};
use gglib_core::services::total_model_bytes;

/// The KV cache types a model is loaded with: the ones [`fit_verdict`] sizes
/// the context at, so the cache the fit paid for is the cache that is made.
pub(crate) const CACHE_TYPES: (KvCacheType, KvCacheType) =
    (DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V);

/// The context to load `spec` with inside `budget_bytes`: the largest rung
/// of the context ladder that fits, lowered to `asked` when that is smaller
/// and not zero. A context asked for never raises it.
///
/// The fit is [`fit_verdict`]'s on the weights as they are on disk, every
/// shard of them ([`total_model_bytes`]). That is all the engine loads, and
/// the figure the model browser judges an installed model by; a projector or
/// a component the catalog counts beside the weights is not loaded, so it is
/// not counted. A model therefore loads at the context the browser showed
/// for the same budget.
///
/// A model whose smallest rung does not fit is refused with
/// [`ModelRuntimeError::ChatModelDoesNotFit`], and one trained for fewer
/// tokens than the smallest rung, which the fit serves no context, with
/// words that say so. One whose fit cannot be judged, because its GGUF lacks
/// a fact the fit needs, its weights cannot be read, or the device's memory
/// could not be read, is refused too, naming the missing input: on a phone
/// a load that does not fit ends the app, not the load.
pub(crate) async fn context_for(
    spec: &ModelLaunchSpec,
    budget_bytes: Option<u64>,
    asked: Option<u64>,
) -> Result<u32, ModelRuntimeError> {
    let path = spec.file_path.clone();
    // A size that cannot be read is 0, which the fit takes as unknown.
    let weights = tokio::task::spawn_blocking(move || total_model_bytes(&path))
        .await
        .unwrap_or(0);
    context_within(spec, weights, budget_bytes, asked)
}

/// [`context_for`], for a model whose weights are `weights_bytes`.
fn context_within(
    spec: &ModelLaunchSpec,
    weights_bytes: u64,
    budget_bytes: Option<u64>,
    asked: Option<u64>,
) -> Result<u32, ModelRuntimeError> {
    let (verdict, inputs) = fit_verdict(
        spec.context_length,
        Some(weights_bytes),
        spec.kv_elems_per_token,
        budget_bytes,
    );
    let rung = match verdict {
        FitVerdict::Fits { ctx } | FitVerdict::Tight { ctx } => ctx,
        FitVerdict::TooBig {
            need_bytes,
            have_bytes,
        } => {
            return Err(match inputs.trained_ctx {
                // The fit serves no context under the ladder's lowest rung,
                // whatever the memory.
                Some(trained) if trained < DEFAULT_CONTEXT_SIZE => {
                    ModelRuntimeError::SpawnFailed(format!(
                        "'{}' was trained for {trained} tokens, under the \
                         {DEFAULT_CONTEXT_SIZE} this device serves a model at least",
                        spec.name
                    ))
                }
                _ => ModelRuntimeError::ChatModelDoesNotFit {
                    needed_bytes: need_bytes,
                    budget_bytes: have_bytes,
                },
            });
        }
        FitVerdict::Unknown { missing } => {
            return Err(ModelRuntimeError::SpawnFailed(format!(
                "can't tell whether '{}' fits this device: {}",
                spec.name,
                unknown(missing)
            )));
        }
    };
    let n_ctx = asked.filter(|&n| n > 0).map_or(rung, |n| n.min(rung));
    u32::try_from(n_ctx).map_err(|_| {
        ModelRuntimeError::Internal(format!("a context of {n_ctx} tokens is not a u32"))
    })
}

/// What a fit could not be judged without, in words.
fn unknown(missing: MissingInput) -> &'static str {
    match missing {
        MissingInput::TrainedContext => {
            "its GGUF does not say how long a context it was trained for"
        }
        MissingInput::KvShape => "its GGUF does not give the shape of its KV cache",
        MissingInput::Weights => "the size of its weights could not be read",
        MissingInput::Budget => "the device's memory could not be read",
    }
}

#[cfg(test)]
#[path = "fit_tests.rs"]
mod tests;
