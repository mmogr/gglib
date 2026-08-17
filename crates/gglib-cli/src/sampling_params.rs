//! Clearing one sampling parameter by its CLI flag name.
//!
//! Two surfaces store an [`InferenceConfig`] a user edits in place — `gglib
//! config profile set` and `gglib model update` — and both need the same
//! `--unset <param>` verb, because on both of them "set it back to unset" is
//! otherwise inexpressible: the flags carry values, and an absent flag means
//! *not mentioned* rather than *cleared*.
//!
//! One implementation, so the two surfaces cannot accept different names or
//! disagree about which parameters can be cleared. The list here is the whole
//! modelled sampler surface, `seed` excepted — no surface sets a stored seed,
//! so none can clear one either.

use anyhow::{Result, bail};
use gglib_core::domain::InferenceConfig;

/// Clear one parameter by its CLI flag name.
///
/// Accepts either spelling, so `--unset top-k` and `--unset top_k` both work.
///
/// # Errors
///
/// Names the unknown parameter and lists what is accepted.
pub(crate) fn clear_param(config: &mut InferenceConfig, param: &str) -> Result<()> {
    match param.replace('_', "-").as_str() {
        "temperature" => config.temperature = None,
        "top-p" => config.top_p = None,
        "top-k" => config.top_k = None,
        "max-tokens" => config.max_tokens = None,
        "repeat-penalty" => config.repeat_penalty = None,
        "presence-penalty" => config.presence_penalty = None,
        "min-p" => config.min_p = None,
        "frequency-penalty" => config.frequency_penalty = None,
        "dry-multiplier" => config.dry_multiplier = None,
        "dry-base" => config.dry_base = None,
        "dry-allowed-length" => config.dry_allowed_length = None,
        "dry-penalty-last-n" => config.dry_penalty_last_n = None,
        "dynatemp-range" => config.dynatemp_range = None,
        "dynatemp-exponent" => config.dynatemp_exponent = None,
        "top-n-sigma" => config.top_n_sigma = None,
        "reasoning-effort" => config.reasoning_effort = None,
        "reasoning-budget-tokens" => config.reasoning_budget_tokens = None,
        other => bail!(
            "unknown parameter '{other}'; expected one of: temperature, top-p, \
             top-k, max-tokens, repeat-penalty, presence-penalty, min-p, \
             frequency-penalty, dynatemp-range, dynatemp-exponent, top-n-sigma, \
             dry-multiplier, dry-base, dry-allowed-length, dry-penalty-last-n, \
             reasoning-effort, reasoning-budget-tokens"
        ),
    }
    Ok(())
}

#[cfg(test)]
#[path = "sampling_params_tests.rs"]
mod sampling_params_tests;
