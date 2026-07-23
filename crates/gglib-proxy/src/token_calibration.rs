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
use std::sync::Mutex;
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
const SESSION_SNAPSHOT_TTL: Duration = Duration::from_secs(2 * 60 * 60);

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
pub struct TokenCalibration {
    ratios: Mutex<HashMap<String, f64>>,
    session_snapshots: Mutex<SessionSnapshots>,
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

    /// The chars-per-token factor to use for `model` within `session_id`,
    /// frozen at whatever [`Self::chars_per_token`] returned the first time
    /// this (session, model) pair was seen — or the last time it went stale
    /// past [`SESSION_SNAPSHOT_TTL`] — rather than the live, still-adapting
    /// value every other request would read.
    ///
    /// This is what keeps two turns of one conversation from computing two
    /// different truncation budgets purely from the EWMA settling in the
    /// background: [`Self::record`] still updates on every request as before,
    /// but a session that's already snapshotted doesn't see that drift again
    /// until its snapshot expires or is explicitly cleared.
    #[must_use]
    pub fn session_chars_per_token(&self, model: &str, session_id: &str, now: Instant) -> f64 {
        let key = session_key(session_id, model);
        let mut guard = self
            .session_snapshots
            .lock()
            .unwrap_or_else(|e| e.into_inner());

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
    pub fn clear_session(&self, session_id: &str) {
        self.session_snapshots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove_session(session_id);
    }

    /// Drop every frozen snapshot. Called on a wholesale cache clear.
    pub fn clear_all_sessions(&self) {
        *self
            .session_snapshots
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = SessionSnapshots::default();
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

    #[test]
    fn session_snapshot_is_stable_while_the_global_ratio_keeps_drifting() {
        let cal = TokenCalibration::new();
        let t0 = Instant::now();
        cal.record("m", 40_000, 10_000); // global ratio: 4.0
        let frozen = cal.session_chars_per_token("m", "sess-1", t0);

        // More turns land, each updating the global EWMA...
        cal.record("m", 30_000, 10_000);
        cal.record("m", 20_000, 10_000);
        assert_ne!(
            cal.chars_per_token("m"),
            frozen,
            "the global ratio really did move"
        );

        // ...but this session's snapshot must not.
        assert_eq!(cal.session_chars_per_token("m", "sess-1", t0), frozen);
    }

    #[test]
    fn different_sessions_get_independent_snapshots() {
        let cal = TokenCalibration::new();
        let t0 = Instant::now();
        cal.record("m", 40_000, 10_000); // 4.0
        let s1 = cal.session_chars_per_token("m", "sess-1", t0);

        cal.record("m", 20_000, 10_000); // pulls global ratio down
        let s2 = cal.session_chars_per_token("m", "sess-2", t0);

        assert_eq!(s1, 4.0);
        assert!(
            s2 < 4.0,
            "sess-2's first snapshot should see the drifted ratio"
        );
        assert_eq!(
            cal.session_chars_per_token("m", "sess-1", t0),
            s1,
            "sess-1 unaffected"
        );
    }

    #[test]
    fn session_snapshot_is_keyed_per_model() {
        let cal = TokenCalibration::new();
        let t0 = Instant::now();
        cal.record("model-a", 40_000, 10_000); // 4.0
        cal.record("model-b", 20_000, 10_000); // 2.0
        assert_eq!(cal.session_chars_per_token("model-a", "sess-1", t0), 4.0);
        assert_eq!(cal.session_chars_per_token("model-b", "sess-1", t0), 2.0);
    }

    #[test]
    fn clear_session_forces_a_fresh_snapshot() {
        let cal = TokenCalibration::new();
        let t0 = Instant::now();
        cal.record("m", 40_000, 10_000);
        let before = cal.session_chars_per_token("m", "sess-1", t0);

        cal.record("m", 20_000, 10_000); // drift the global ratio
        cal.clear_session("sess-1");
        let after = cal.session_chars_per_token("m", "sess-1", t0);

        assert_ne!(
            before, after,
            "clearing must pick up the drifted global ratio"
        );
    }

    #[test]
    fn clear_all_sessions_resets_every_session() {
        let cal = TokenCalibration::new();
        let t0 = Instant::now();
        cal.record("m", 40_000, 10_000);
        let _ = cal.session_chars_per_token("m", "sess-1", t0);
        let _ = cal.session_chars_per_token("m", "sess-2", t0);

        cal.record("m", 20_000, 10_000);
        cal.clear_all_sessions();

        let refreshed = cal.session_chars_per_token("m", "sess-1", t0 + Duration::from_secs(1));
        assert_ne!(refreshed, 4.0);
    }

    #[test]
    fn session_snapshot_expires_after_the_ttl() {
        let cal = TokenCalibration::new();
        let t0 = Instant::now();
        cal.record("m", 40_000, 10_000); // 4.0
        let frozen = cal.session_chars_per_token("m", "sess-1", t0);

        cal.record("m", 20_000, 10_000); // drift while "frozen"
        let still_frozen = cal.session_chars_per_token("m", "sess-1", t0 + Duration::from_secs(60));
        assert_eq!(still_frozen, frozen, "well within the TTL");

        let after_ttl = cal.session_chars_per_token(
            "m",
            "sess-1",
            t0 + SESSION_SNAPSHOT_TTL + Duration::from_secs(1),
        );
        assert_ne!(
            after_ttl, frozen,
            "TTL expiry must pick up the drifted ratio"
        );
    }

    #[test]
    fn session_snapshots_are_bounded_by_max_session_snapshots() {
        let cal = TokenCalibration::new();
        let t0 = Instant::now();
        cal.record("m", 40_000, 10_000);
        for i in 0..(MAX_SESSION_SNAPSHOTS + 10) {
            let _ = cal.session_chars_per_token("m", &format!("sess-{i}"), t0);
        }
        let guard = cal.session_snapshots.lock().unwrap();
        assert!(guard.values.len() <= MAX_SESSION_SNAPSHOTS);
    }
}
