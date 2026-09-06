//! The inference-parameter validators, split from [`super`](crate::settings).
//!
//! Moved here whole, unchanged, so `settings.rs` could take the remote
//! tunnel's two fields without growing past its ceiling. `validate_settings`
//! stays beside the struct it validates and reaches these through the
//! re-export; every external caller does too, so the module path is a detail.

use crate::domain::{InferenceConfig, InferenceProfile};

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
/// Checks that all specified parameters are within valid ranges.
pub fn validate_inference_config(config: &InferenceConfig) -> Result<(), String> {
    // Validate temperature (0.0 - 2.0)
    if let Some(temp) = config.temperature
        && !(0.0..=2.0).contains(&temp)
    {
        return Err(format!(
            "Temperature must be between 0.0 and 2.0, got {temp}"
        ));
    }

    // Validate top_p (0.0 - 1.0)
    if let Some(top_p) = config.top_p
        && !(0.0..=1.0).contains(&top_p)
    {
        return Err(format!("Top P must be between 0.0 and 1.0, got {top_p}"));
    }

    // Validate top_k (must be positive)
    if let Some(top_k) = config.top_k
        && top_k <= 0
    {
        return Err(format!("Top K must be positive, got {top_k}"));
    }

    // Validate max_tokens (must be positive)
    if let Some(max_tokens) = config.max_tokens
        && max_tokens == 0
    {
        return Err("Max tokens must be positive".to_string());
    }

    // Validate reasoning_budget_tokens (>= -1, exactly upstream's range —
    // llama-server answers -2 with an HTTP 400 naming it, ADR 0007 finding 7c;
    // -1 defers to the launch `--reasoning-budget` and 0 stops thinking).
    //
    // This guard is the *stored* half of a boundary the request half already
    // has. `InferenceConfig::extract_client_sampling` applies the same range to
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
        && budget < -1
    {
        return Err(format!(
            "Reasoning budget tokens must be -1 or greater \
             (-1 defers to the launch default, 0 stops thinking), got {budget}"
        ));
    }

    // Validate repeat_penalty (must be positive)
    if let Some(repeat_penalty) = config.repeat_penalty
        && repeat_penalty <= 0.0
    {
        return Err(format!(
            "Repeat penalty must be positive, got {repeat_penalty}"
        ));
    }

    // Validate presence_penalty (0.0 - 2.0)
    if let Some(pp) = config.presence_penalty
        && !(0.0..=2.0).contains(&pp)
    {
        return Err(format!(
            "Presence penalty must be between 0.0 and 2.0, got {pp}"
        ));
    }

    // Validate min_p (0.0 - 1.0)
    if let Some(mp) = config.min_p
        && !(0.0..=1.0).contains(&mp)
    {
        return Err(format!("Min P must be between 0.0 and 1.0, got {mp}"));
    }

    // Validate frequency_penalty (-2.0 - 2.0, the OpenAI-spec range llama.cpp
    // honours; negative values encourage reuse and are valid upstream)
    if let Some(fp) = config.frequency_penalty
        && !(-2.0..=2.0).contains(&fp)
    {
        return Err(format!(
            "Frequency penalty must be between -2.0 and 2.0, got {fp}"
        ));
    }

    // Validate dynatemp_range (non-negative; 0.0 disables dynamic temperature)
    if let Some(dr) = config.dynatemp_range
        && dr < 0.0
    {
        return Err(format!(
            "Dynatemp range must be non-negative (0.0 disables), got {dr}"
        ));
    }

    // Validate dynatemp_exponent (must be positive; inert without a range)
    if let Some(de) = config.dynatemp_exponent
        && de <= 0.0
    {
        return Err(format!("Dynatemp exponent must be positive, got {de}"));
    }

    // Validate top_n_sigma (-1.0 disables; llama.cpp treats any value at or
    // below zero as off, and -1.0 is its own spelling of the default)
    if let Some(ts) = config.top_n_sigma
        && ts < -1.0
    {
        return Err(format!(
            "Top-n-sigma must be -1.0 (disabled) or greater, got {ts}"
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
    // Validate dry_multiplier (0.0 - 5.0; 0.0 disables DRY)
    if let Some(dm) = config.dry_multiplier
        && !(0.0..=5.0).contains(&dm)
    {
        return Err(format!(
            "DRY multiplier must be between 0.0 and 5.0, got {dm}"
        ));
    }

    // Validate dry_base (> 1.0; the exponent base grows the penalty with
    // matched sequence length, so a base at or below 1.0 cannot penalise)
    if let Some(db) = config.dry_base
        && db <= 1.0
    {
        return Err(format!("DRY base must be greater than 1.0, got {db}"));
    }

    // Validate dry_allowed_length (non-negative token count)
    if let Some(dal) = config.dry_allowed_length
        && dal < 0
    {
        return Err(format!(
            "DRY allowed length must be non-negative, got {dal}"
        ));
    }

    // Validate dry_penalty_last_n (0 disables; negatives are resolved by
    // llama.cpp against the context size)
    if let Some(dpn) = config.dry_penalty_last_n
        && dpn < -1
    {
        return Err(format!(
            "DRY penalty last N must be -1 or greater (0 disables), got {dpn}"
        ));
    }

    Ok(())
}
