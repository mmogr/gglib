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

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use gglib_core::request_pipeline::CHARS_PER_TOKEN_APPROX;

/// EWMA smoothing factor applied to each new observation (`0.0..=1.0`). Higher
/// reacts faster; lower is steadier. 0.2 blends ~5 recent requests.
const EWMA_ALPHA: f64 = 0.2;

/// Lower/upper clamp on any observed or stored ratio, guarding against
/// pathological single requests (e.g. a tiny prompt with a large fixed
/// template) skewing the budget.
const MIN_RATIO: f64 = 2.0;
const MAX_RATIO: f64 = 8.0;

/// Maximum number of distinct sessions [`TokenCalibration`] remembers a
/// frozen snapshot for. Bounded the same way `ContextMetricsStore`'s ring
/// buffer is (`crate::metrics::MAX_SNAPSHOTS`) — oldest evicted first,
/// rather than growing without limit for the life of the process.
const MAX_SESSION_SNAPSHOTS: usize = 256;

/// How long a frozen per-session snapshot is trusted before a request for
/// that session re-baselines it from the live ratio.
///
/// Long enough that no realistic back-to-back exchange within one chat
/// session — the turn-to-turn cadence this snapshot exists to protect —
/// ever crosses it; short enough that a session left open for hours
/// eventually re-aligns with reality instead of carrying a first-request
/// guess forever.
const SESSION_SNAPSHOT_TTL: Duration = Duration::from_hours(2);

/// A chars-per-token ratio frozen at a point in time for one session.
#[derive(Debug, Clone, Copy)]
struct SessionSnapshot {
    ratio: f64,
    taken_at: Instant,
}

/// FIFO-bounded map: same eviction shape as `ContextMetricsStore`'s ring
/// buffer, applied to session ids instead of per-request snapshots.
#[derive(Debug, Default)]
struct SessionSnapshots {
    values: HashMap<String, SessionSnapshot>,
    order: VecDeque<String>,
}

impl SessionSnapshots {
    fn insert(&mut self, key: String, snap: SessionSnapshot) {
        if !self.values.contains_key(&key) {
            self.order.push_back(key.clone());
            if self.order.len() > MAX_SESSION_SNAPSHOTS
                && let Some(oldest) = self.order.pop_front()
            {
                self.values.remove(&oldest);
            }
        }
        self.values.insert(key, snap);
    }

    fn remove_session(&mut self, session_id: &str) {
        let prefix = format!("{session_id}\u{0}");
        self.values.retain(|k, _| !k.starts_with(&prefix));
        self.order.retain(|k| !k.starts_with(&prefix));
    }
}

/// Composite key: a session that switches models mid-conversation must
/// re-snapshot rather than reuse a ratio learned for a different tokenizer.
fn session_key(session_id: &str, model: &str) -> String {
    format!("{session_id}\u{0}{model}")
}

/// Per-model rolling chars-per-token estimator.
///
/// Wrap in `Arc` and share across handler tasks.
#[derive(Debug, Default)]
pub(crate) struct TokenCalibration {
    ratios: Mutex<HashMap<String, f64>>,
    session_snapshots: Mutex<SessionSnapshots>,
}

impl TokenCalibration {
    /// Create an empty calibrator (every model falls back to the static
    /// default until it sees its first observation).
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Fold one observation into `model`'s rolling ratio.
    ///
    /// `payload_chars` is the size of the body actually forwarded upstream,
    /// or `None` for a request that carried an image: an image's tokens come
    /// from its pixels, so such a request's characters say nothing about the
    /// tokenizer and it is ignored. `prompt_tokens` is the count llama.cpp
    /// reported for it; a zero count is ignored, and an out-of-range ratio is
    /// clamped.
    pub(crate) fn record(&self, model: &str, payload_chars: Option<usize>, prompt_tokens: u32) {
        let Some(payload_chars) = payload_chars else {
            return;
        };
        if prompt_tokens == 0 {
            return;
        }
        let observed =
            (payload_chars as f64 / f64::from(prompt_tokens)).clamp(MIN_RATIO, MAX_RATIO);

        let mut guard = self.ratios.lock().unwrap_or_else(PoisonError::into_inner);
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
    pub(crate) fn chars_per_token(&self, model: &str) -> f64 {
        let guard = self.ratios.lock().unwrap_or_else(PoisonError::into_inner);
        guard
            .get(model)
            .copied()
            .unwrap_or(CHARS_PER_TOKEN_APPROX as f64)
    }

    /// The chars-per-token factor to use for `model` within `session_id`,
    /// frozen at whatever [`Self::chars_per_token`] returned the first time
    /// this (session, model) pair was seen — or the last time it went stale
    /// past [`SESSION_SNAPSHOT_TTL`] — rather than the live, still-adapting
    /// value every other request would read.
    ///
    /// This is what keeps two turns of one conversation from computing two
    /// different truncation budgets purely from the EWMA settling in the
    /// background: [`Self::record`] updates on every request,
    /// but a session that's already snapshotted doesn't see that drift again
    /// until its snapshot expires or is explicitly cleared.
    #[must_use]
    pub(crate) fn session_chars_per_token(
        &self,
        model: &str,
        session_id: &str,
        now: Instant,
    ) -> f64 {
        let key = session_key(session_id, model);
        let mut guard = self
            .session_snapshots
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        if let Some(snap) = guard.values.get(&key)
            && now.duration_since(snap.taken_at) < SESSION_SNAPSHOT_TTL
        {
            return snap.ratio;
        }

        // Different mutex (`self.ratios`) — no self-deadlock nesting this
        // call inside the `session_snapshots` guard.
        let ratio = self.chars_per_token(model);
        guard.insert(
            key,
            SessionSnapshot {
                ratio,
                taken_at: now,
            },
        );
        ratio
    }

    /// Drop the frozen snapshot(s) for `session_id` (all models), so the next
    /// request for it re-baselines from the current live ratio. Called when
    /// that session's cache is explicitly cleared.
    pub(crate) fn clear_session(&self, session_id: &str) {
        self.session_snapshots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove_session(session_id);
    }

    /// Drop every frozen snapshot. Called on a wholesale cache clear.
    pub(crate) fn clear_all_sessions(&self) {
        *self
            .session_snapshots
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = SessionSnapshots::default();
    }
}

#[cfg(test)]
#[path = "token_calibration_tests.rs"]
mod token_calibration_tests;
