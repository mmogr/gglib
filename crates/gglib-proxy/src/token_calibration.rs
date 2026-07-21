//! Per-model chars-per-token calibration for the truncation budget.
//!
//! The truncation budget converts a **token** context size (`effective_ctx`)
//! into a **character** budget by multiplying by a chars-per-token factor. The
//! static default ([`CHARS_PER_TOKEN_APPROX`] = 4) is
//! deliberately matched to the VS Code LLM Gateway's own estimate, but real
//! code/markup content tokenizes closer to ~3.3 chars/token, so the static
//! factor *overestimates* the character budget and can let an over-long prompt
//! through to the upstream.
//!
//! [`TokenCalibration`] closes the loop: every streamed response carries a
//! `usage.prompt_tokens` count from llama.cpp. Paired with the number of
//! characters actually forwarded, that yields an observed chars-per-token
//! ratio for the model, smoothed with an exponentially-weighted moving average
//! (EWMA). Subsequent requests use the calibrated ratio, so the budget tracks
//! the model's real tokenizer instead of a fixed guess.
//!
//! ## Concurrency design
//!
//! `std::sync::Mutex` around a small `HashMap`; every critical section is a
//! couple of map operations with no `.await`, matching the lock discipline of
//! [`crate::metrics::ContextMetricsStore`].

use std::collections::HashMap;
use std::sync::Mutex;

use gglib_core::request_pipeline::CHARS_PER_TOKEN_APPROX;

/// EWMA smoothing factor applied to each new observation (`0.0..=1.0`). Higher
/// reacts faster; lower is steadier. 0.2 blends ~5 recent requests.
const EWMA_ALPHA: f64 = 0.2;

/// Lower/upper clamp on any observed or stored ratio, guarding against
/// pathological single requests (e.g. a tiny prompt with a large fixed
/// template) skewing the budget.
const MIN_RATIO: f64 = 2.0;
const MAX_RATIO: f64 = 8.0;

/// Per-model rolling chars-per-token estimator.
///
/// Wrap in `Arc` and share across handler tasks.
#[derive(Debug, Default)]
pub struct TokenCalibration {
    ratios: Mutex<HashMap<String, f64>>,
}

impl TokenCalibration {
    /// Create an empty calibrator (every model falls back to the static
    /// default until it sees its first observation).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one observation into `model`'s rolling ratio.
    ///
    /// `payload_chars` is the size of the body actually forwarded upstream;
    /// `prompt_tokens` is the count llama.cpp reported for it. A zero token
    /// count (or an out-of-range ratio) is ignored.
    pub fn record(&self, model: &str, payload_chars: usize, prompt_tokens: u32) {
        if prompt_tokens == 0 {
            return;
        }
        let observed =
            (payload_chars as f64 / f64::from(prompt_tokens)).clamp(MIN_RATIO, MAX_RATIO);

        let mut guard = self.ratios.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .entry(model.to_owned())
            .and_modify(|current| {
                *current = (1.0 - EWMA_ALPHA) * *current + EWMA_ALPHA * observed;
            })
            .or_insert(observed);
    }

    /// The chars-per-token factor to use for `model`, or the static default
    /// ([`CHARS_PER_TOKEN_APPROX`]) if the model has no observations yet.
    #[must_use]
    pub fn chars_per_token(&self, model: &str) -> f64 {
        let guard = self.ratios.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .get(model)
            .copied()
            .unwrap_or(CHARS_PER_TOKEN_APPROX as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_model_returns_static_default() {
        let cal = TokenCalibration::new();
        assert!((cal.chars_per_token("nope") - CHARS_PER_TOKEN_APPROX as f64).abs() < f64::EPSILON);
    }

    #[test]
    fn first_observation_sets_ratio() {
        let cal = TokenCalibration::new();
        // 33000 chars / 10000 tokens = 3.3 chars/token.
        cal.record("m", 33_000, 10_000);
        assert!((cal.chars_per_token("m") - 3.3).abs() < 1e-9);
    }

    #[test]
    fn zero_tokens_ignored() {
        let cal = TokenCalibration::new();
        cal.record("m", 5_000, 0);
        assert!((cal.chars_per_token("m") - CHARS_PER_TOKEN_APPROX as f64).abs() < f64::EPSILON);
    }

    #[test]
    fn ewma_moves_toward_new_observations() {
        let cal = TokenCalibration::new();
        cal.record("m", 40_000, 10_000); // 4.0
        cal.record("m", 30_000, 10_000); // 3.0 → EWMA 0.8*4 + 0.2*3 = 3.8
        let r = cal.chars_per_token("m");
        assert!(r < 4.0 && r > 3.0, "ratio {r} should move toward 3.0");
    }

    #[test]
    fn ratio_is_clamped_to_sane_bounds() {
        let cal = TokenCalibration::new();
        // 100 chars / 1 token = 100 → clamped to MAX_RATIO.
        cal.record("hi", 100, 1);
        assert!((cal.chars_per_token("hi") - MAX_RATIO).abs() < f64::EPSILON);
        // 1 char / 1000 tokens ≈ 0 → clamped to MIN_RATIO.
        cal.record("lo", 1, 1_000);
        assert!((cal.chars_per_token("lo") - MIN_RATIO).abs() < f64::EPSILON);
    }
}
