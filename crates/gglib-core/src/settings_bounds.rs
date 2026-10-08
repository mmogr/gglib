//! What each bounded setting accepts, stated once.
//!
//! One constant per bound, because more than one surface describes each and
//! they have to agree. [`validate_settings`](super::validate_settings) and
//! [`validate_inference_config`](super::validate_inference_config) check
//! against these and format what they refuse with from them; the help of
//! `gglib config settings set` is held to them by a test beside its flags; and
//! `contracts/settings/bounds.json`, which the settings page's own ranges are
//! checked against, is written from them by this module's tests. Spelling a
//! number out separately on each is how they drift.
//!
//! A `*_RANGE` is closed at both ends. [`MIN_PORT`] and each `*_MIN` is the
//! lowest value accepted, and an `*_EXCLUSIVE_MIN` the highest one refused;
//! none of those has a ceiling. [`MAX_DEVICE_ID_LEN`] is a length, not a value.

use std::ops::RangeInclusive;

// ── Settings ────────────────────────────────────────────────────────

/// The context sizes a person is allowed to configure.
///
/// [`validate_settings`](super::validate_settings) rejects anything outside
/// it, and so do the flags that write this setting or default it.
///
/// Not every context-size flag is bounded by it: `--ctx-size` names a
/// per-launch value rather than this setting, and `CtxSizeArg::parse` accepts
/// any `u64`. That is a separate surface with a separate contract, not an
/// omission here.
pub const CONTEXT_SIZE_RANGE: RangeInclusive<u64> = 512..=1_000_000;

/// The lowest port `proxy_port` and `llama_base_port` may name. Anything
/// below it is privileged, and binding one takes root.
pub const MIN_PORT: u16 = 1024;

/// How many downloads `max_download_queue_size` may let queue.
pub const DOWNLOAD_QUEUE_RANGE: RangeInclusive<u32> = 1..=50;

/// The longest id a row of `remote_devices` may carry, in bytes: modelpipe's
/// limit on the name it holds a token under.
pub const MAX_DEVICE_ID_LEN: usize = 64;

// ── Inference parameters ────────────────────────────────────────────

/// `temperature`.
pub const TEMPERATURE_RANGE: RangeInclusive<f32> = 0.0..=2.0;

/// `top_p`.
pub const TOP_P_RANGE: RangeInclusive<f32> = 0.0..=1.0;

/// `top_k`.
pub const TOP_K_MIN: i32 = 1;

/// `max_tokens`.
pub const MAX_TOKENS_MIN: u32 = 1;

/// `reasoning_budget_tokens`: exactly upstream's floor.
///
/// llama-server answers anything below it with an HTTP 400 naming the range
/// (ADR 0007 finding 7c); the floor itself defers to the launch
/// `--reasoning-budget`, and `0` stops thinking. A budget that arrives on a
/// request is held to it too, by
/// [`InferenceConfig::extract_client_sampling`](crate::domain::InferenceConfig::extract_client_sampling).
pub const REASONING_BUDGET_TOKENS_MIN: i32 = -1;

/// `repeat_penalty`.
pub const REPEAT_PENALTY_EXCLUSIVE_MIN: f32 = 0.0;

/// `presence_penalty`.
pub const PRESENCE_PENALTY_RANGE: RangeInclusive<f32> = 0.0..=2.0;

/// `min_p`.
pub const MIN_P_RANGE: RangeInclusive<f32> = 0.0..=1.0;

/// `frequency_penalty`: the OpenAI-spec range llama.cpp honours. Negative
/// values encourage reuse and are valid upstream.
pub const FREQUENCY_PENALTY_RANGE: RangeInclusive<f32> = -2.0..=2.0;

/// `dynatemp_range`: the floor itself turns dynamic temperature off.
pub const DYNATEMP_RANGE_MIN: f32 = 0.0;

/// `dynatemp_exponent`: inert without a `dynatemp_range`.
pub const DYNATEMP_EXPONENT_EXCLUSIVE_MIN: f32 = 0.0;

/// `top_n_sigma`: llama.cpp reads any value at or below zero as off, and the
/// floor is its own spelling of the default.
pub const TOP_N_SIGMA_MIN: f32 = -1.0;

/// `dry_multiplier`: the low end turns DRY off.
pub const DRY_MULTIPLIER_RANGE: RangeInclusive<f32> = 0.0..=5.0;

/// `dry_base`: the exponent base grows the penalty with the length of the
/// matched sequence, so a base at or below this cannot penalise.
pub const DRY_BASE_EXCLUSIVE_MIN: f32 = 1.0;

/// `dry_allowed_length`: a token count.
pub const DRY_ALLOWED_LENGTH_MIN: i32 = 0;

/// `dry_penalty_last_n`: `0` turns the scan off, and llama.cpp resolves the
/// floor against the context size.
pub const DRY_PENALTY_LAST_N_MIN: i32 = -1;

#[cfg(test)]
#[path = "settings_bounds_tests.rs"]
mod tests;
