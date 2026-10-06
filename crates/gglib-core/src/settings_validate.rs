//! The inference-parameter validators, split from [`super`](crate::settings).
//!
//! Moved here whole, unchanged, so `settings.rs` could take the remote
//! tunnel's two fields without growing past its ceiling. `validate_settings`
//! stays beside the struct it validates and reaches these through the
//! re-export; every external caller does too, so the module path is a detail.

use std::fmt::{Debug, Display};
use std::ops::RangeInclusive;

use crate::domain::{InferenceConfig, InferenceProfile};

use super::settings_bounds::{
    DRY_ALLOWED_LENGTH_MIN, DRY_BASE_EXCLUSIVE_MIN, DRY_MULTIPLIER_RANGE, DRY_PENALTY_LAST_N_MIN,
    DYNATEMP_EXPONENT_EXCLUSIVE_MIN, DYNATEMP_RANGE_MIN, FREQUENCY_PENALTY_RANGE, MAX_TOKENS_MIN,
    MIN_P_RANGE, PRESENCE_PENALTY_RANGE, REASONING_BUDGET_TOKENS_MIN, REPEAT_PENALTY_EXCLUSIVE_MIN,
    TEMPERATURE_RANGE, TOP_K_MIN, TOP_N_SIGMA_MIN, TOP_P_RANGE,
};

/// Validate a set of inference profiles.
///
/// Checks each profile's name against
/// [`crate::domain::inference_profile::validate_name`], rejects
/// duplicate names (they would make `{model}:{profile}` ambiguous), and reuses
/// [`validate_inference_config`] for the numeric ranges so profile parameters
/// and global defaults can never drift apart on what counts as valid.
///
/// # Errors
///
/// Returns a human-readable description of the first problem found.
pub fn validate_inference_profiles(profiles: &[InferenceProfile]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::with_capacity(profiles.len());

    for profile in profiles {
        profile.validate().map_err(|e| e.to_string())?;

        if seen.contains(&profile.name.as_str()) {
            return Err(format!("duplicate profile name '{}'", profile.name));
        }
        seen.push(&profile.name);

        validate_inference_config(&profile.config)
            .map_err(|e| format!("profile '{}': {e}", profile.name))?;
    }

    Ok(())
}

/// Validate inference configuration parameters.
///
/// Checks that all specified parameters are within valid ranges. Each bound is
/// a constant in [`settings_bounds`](super::settings_bounds), and what a
/// refusal says is formatted from the same constant.
pub fn validate_inference_config(config: &InferenceConfig) -> Result<(), String> {
    if let Some(temp) = config.temperature
        && !TEMPERATURE_RANGE.contains(&temp)
    {
        return Err(outside("Temperature", &TEMPERATURE_RANGE, temp));
    }

    if let Some(top_p) = config.top_p
        && !TOP_P_RANGE.contains(&top_p)
    {
        return Err(outside("Top P", &TOP_P_RANGE, top_p));
    }

    if let Some(top_k) = config.top_k
        && top_k < TOP_K_MIN
    {
        return Err(format!("Top K must be {TOP_K_MIN} or greater, got {top_k}"));
    }

    if let Some(max_tokens) = config.max_tokens
        && max_tokens < MAX_TOKENS_MIN
    {
        return Err(format!("Max tokens must be {MAX_TOKENS_MIN} or greater"));
    }

    // This guard is the *stored* half of a boundary the request half already
    // has. `InferenceConfig::extract_client_sampling` applies the same floor to
    // a value that arrives on a request, but three surfaces deserialise a whole
    // `InferenceConfig` and never pass through it: `Settings::inference_defaults`,
    // `inference_profiles[].config`, and the proxy's `inference_override`. A
    // value stored through any of them is force-inserted into every chat body,
    // so `-5000` in global defaults means an HTTP 400 on every request to every
    // model until someone finds the setting — and neither reasoning control is
    // observable in `/slots` or `/props` (ADR 0007 finding 7a), so no readback
    // can ever point at it. Rejecting at store time is the only place this is
    // catchable.
    //
    // `reasoning_effort` needs no twin guard: it is an enum, so serde refuses
    // an unknown level before this function is reached.
    if let Some(budget) = config.reasoning_budget_tokens
        && budget < REASONING_BUDGET_TOKENS_MIN
    {
        return Err(format!(
            "Reasoning budget tokens must be {REASONING_BUDGET_TOKENS_MIN} or greater \
             (-1 defers to the launch default, 0 stops thinking), got {budget}"
        ));
    }

    if let Some(repeat_penalty) = config.repeat_penalty
        && repeat_penalty <= REPEAT_PENALTY_EXCLUSIVE_MIN
    {
        return Err(format!(
            "Repeat penalty must be greater than {REPEAT_PENALTY_EXCLUSIVE_MIN:?}, \
             got {repeat_penalty}"
        ));
    }

    if let Some(pp) = config.presence_penalty
        && !PRESENCE_PENALTY_RANGE.contains(&pp)
    {
        return Err(outside("Presence penalty", &PRESENCE_PENALTY_RANGE, pp));
    }

    if let Some(mp) = config.min_p
        && !MIN_P_RANGE.contains(&mp)
    {
        return Err(outside("Min P", &MIN_P_RANGE, mp));
    }

    if let Some(fp) = config.frequency_penalty
        && !FREQUENCY_PENALTY_RANGE.contains(&fp)
    {
        return Err(outside("Frequency penalty", &FREQUENCY_PENALTY_RANGE, fp));
    }

    if let Some(dr) = config.dynatemp_range
        && dr < DYNATEMP_RANGE_MIN
    {
        return Err(format!(
            "Dynatemp range must be {DYNATEMP_RANGE_MIN:?} or greater (0.0 disables), got {dr}"
        ));
    }

    if let Some(de) = config.dynatemp_exponent
        && de <= DYNATEMP_EXPONENT_EXCLUSIVE_MIN
    {
        return Err(format!(
            "Dynatemp exponent must be greater than {DYNATEMP_EXPONENT_EXCLUSIVE_MIN:?}, got {de}"
        ));
    }

    if let Some(ts) = config.top_n_sigma
        && ts < TOP_N_SIGMA_MIN
    {
        return Err(format!(
            "Top-n-sigma must be {TOP_N_SIGMA_MIN:?} (disabled) or greater, got {ts}"
        ));
    }

    validate_dry_params(config)
}

/// The four DRY parameters' ranges, split out of [`validate_inference_config`].
///
/// Not a judgement about them — they are checked exactly as before and in the
/// same order. They are simply the one cohesive group in a function that is
/// otherwise one field per check, so lifting them is what kept the parent
/// under `clippy::too_many_lines` when `reasoning_budget_tokens` joined. Every
/// caller reaches this through the parent; nothing validates DRY alone.
fn validate_dry_params(config: &InferenceConfig) -> Result<(), String> {
    if let Some(dm) = config.dry_multiplier
        && !DRY_MULTIPLIER_RANGE.contains(&dm)
    {
        return Err(outside("DRY multiplier", &DRY_MULTIPLIER_RANGE, dm));
    }

    if let Some(db) = config.dry_base
        && db <= DRY_BASE_EXCLUSIVE_MIN
    {
        return Err(format!(
            "DRY base must be greater than {DRY_BASE_EXCLUSIVE_MIN:?}, got {db}"
        ));
    }

    if let Some(dal) = config.dry_allowed_length
        && dal < DRY_ALLOWED_LENGTH_MIN
    {
        return Err(format!(
            "DRY allowed length must be {DRY_ALLOWED_LENGTH_MIN} or greater, got {dal}"
        ));
    }

    if let Some(dpn) = config.dry_penalty_last_n
        && dpn < DRY_PENALTY_LAST_N_MIN
    {
        return Err(format!(
            "DRY penalty last N must be {DRY_PENALTY_LAST_N_MIN} or greater (0 disables), \
             got {dpn}"
        ));
    }

    Ok(())
}

/// What a value outside a closed range is refused with.
fn outside<T: Debug + Display>(name: &str, range: &RangeInclusive<T>, got: T) -> String {
    format!(
        "{name} must be between {:?} and {:?}, got {got}",
        range.start(),
        range.end()
    )
}

#[cfg(test)]
#[path = "settings_validate_tests.rs"]
mod tests;
