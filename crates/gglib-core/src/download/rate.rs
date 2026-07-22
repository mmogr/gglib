//! Time-decayed download rate and ETA estimation.
//!
//! This is the single owner of all download speed / ETA math. The download
//! manager owns one [`RateEstimator`] per shard group and ships the values it
//! produces on the wire; every renderer (CLI progress bars, Tauri GUI, web UI)
//! displays those values verbatim. Renderers must never re-derive a rate from
//! byte deltas — doing so is what let the CLI and the GUI disagree with each
//! other, and with the operating system's own network monitor.
//!
//! # Why not an exponentially weighted average of instantaneous rates?
//!
//! Progress arrives in bursts. On the `hf-xet` fast path the byte counter comes
//! from `stat`ing the partially-written file, and the chunk cache flushes to
//! disk in large steps even while the network rate is perfectly flat. Dividing
//! a burst by the short interval it landed in yields an enormous instantaneous
//! rate, and weighting that into a running average still leaks a visible spike.
//!
//! Instead this decays *bytes* and *elapsed time* separately and reports their
//! ratio:
//!
//! ```text
//! decay       = exp(-dt / TAU)
//! accum_bytes = accum_bytes * decay + delta_bytes
//! accum_time  = accum_time  * decay + dt
//! rate        = accum_bytes / accum_time
//! ```
//!
//! A burst adds to the numerator *and* the denominator, so it contributes
//! exactly its own weight and can never spike. Intervals that carry no bytes
//! still add to `accum_time`, so a stall decays the reported rate toward zero
//! rather than freezing it at the last value seen.
//!
//! Two further properties matter for how the manager drives this:
//!
//! * The first sample only establishes a baseline and yields no rate. A resumed
//!   download reports its whole on-disk size in the first event; counting that
//!   as bytes transferred "just now" is what produced multi-GB/s readings.
//! * A byte count that moves backwards re-baselines instead of underflowing.
//!   Per-shard counters restart at zero on every shard, so the manager feeds
//!   aggregate bytes; this is the safety net for the fallback path where shard
//!   sizes are unknown and the aggregate is not monotonic.

use std::time::Instant;

/// Time constant for the rate average.
///
/// The reported rate reflects roughly the last `RATE_TAU` seconds of transfer.
/// This has to be long: `hf-xet` can flush to disk only every couple of
/// seconds, and the residual ripple for a burst arriving every `P` seconds is
/// approximately `±P / (2 * RATE_TAU)`. At 15s a 2-second burst period ripples
/// by under 7%, which reads as steady; at 5s the same input ripples by 20%,
/// which is exactly the jitter this module exists to remove.
///
/// 15s is also what `indicatif`'s own estimator uses for its weighting horizon.
/// The cost is response time: a genuine change in throughput is tracked with a
/// 15s time constant, which is imperceptible against a multi-minute download.
const RATE_TAU: f64 = 15.0;

/// Time constant for the ETA average.
///
/// Shorter than [`RATE_TAU`]: the ETA already inherits that smoothing through
/// the rate it divides by, and this only removes the last of the twitch caused
/// by the remaining-bytes term.
const ETA_TAU: f64 = 5.0;

/// Minimum accumulated observation time before any rate is reported.
///
/// Below this the average is dominated by whatever the first interval happened
/// to contain, so [`RateEstimator::rate_bps`] reports `None` and callers render
/// a placeholder instead of a number that is about to change by an order of
/// magnitude.
const WARMUP_SECS: f64 = 1.5;

/// Time-decayed estimate of download throughput and time remaining.
///
/// Feed it cumulative byte counts with [`record`](Self::record) — on *every*
/// tick, including ticks where the count has not changed, since those are what
/// make a stalled transfer decay toward zero.
#[derive(Debug, Clone)]
pub struct RateEstimator {
    /// Decayed sum of bytes transferred.
    accum_bytes: f64,
    /// Decayed sum of elapsed time, in seconds.
    accum_secs: f64,
    /// Cumulative byte count at the previous sample; `None` before the first.
    prev_bytes: Option<u64>,
    /// Timestamp of the previous sample.
    prev_at: Instant,
    /// Smoothed seconds remaining; `None` when unknown or complete.
    smoothed_eta: Option<f64>,
}

impl RateEstimator {
    /// Create an estimator with its baseline at `now`.
    #[must_use]
    pub const fn new(now: Instant) -> Self {
        Self {
            accum_bytes: 0.0,
            accum_secs: 0.0,
            prev_bytes: None,
            prev_at: now,
            smoothed_eta: None,
        }
    }

    /// Record a cumulative byte count.
    ///
    /// `downloaded` and `total` are cumulative totals for the whole artifact,
    /// not per-tick deltas. `total` may be `0` when the size is not yet known,
    /// in which case no ETA is produced.
    ///
    /// Call this on every tick of the progress bridge. Ticks where `downloaded`
    /// has not moved are meaningful samples: they are how a stall pulls the
    /// reported rate down.
    pub fn record(&mut self, downloaded: u64, total: u64, now: Instant) {
        let dt = now.saturating_duration_since(self.prev_at).as_secs_f64();
        self.prev_at = now;

        let Some(prev) = self.prev_bytes else {
            // First sample: establish the baseline only. Whatever is already on
            // disk was not transferred during this interval.
            self.prev_bytes = Some(downloaded);
            return;
        };

        if downloaded < prev {
            // Counter moved backwards (per-shard counters restart at zero, and
            // the unknown-shard-size fallback is not monotonic). Re-baseline
            // without emitting a sample, keeping the accumulated average so the
            // user sees no discontinuity at a shard boundary.
            self.prev_bytes = Some(downloaded);
            return;
        }

        self.prev_bytes = Some(downloaded);

        if dt > 0.0 {
            let decay = (-dt / RATE_TAU).exp();
            // Byte deltas are far below f64's exact-integer range.
            #[allow(clippy::cast_precision_loss)]
            let delta = (downloaded - prev) as f64;
            self.accum_bytes = self.accum_bytes.mul_add(decay, delta);
            self.accum_secs = self.accum_secs.mul_add(decay, dt);
        }

        self.update_eta(downloaded, total, dt);
    }

    /// Current throughput in bytes per second.
    ///
    /// `None` until enough time has been observed for the average to mean
    /// anything — callers should render a placeholder rather than a zero.
    #[must_use]
    pub fn rate_bps(&self) -> Option<f64> {
        if self.accum_secs < WARMUP_SECS {
            return None;
        }
        let rate = self.accum_bytes / self.accum_secs;
        (rate.is_finite() && rate > 0.0).then_some(rate)
    }

    /// Smoothed estimate of the seconds remaining.
    ///
    /// `None` when the total size is unknown, the transfer is complete, or no
    /// rate has been established yet.
    #[must_use]
    pub const fn eta_seconds(&self) -> Option<f64> {
        self.smoothed_eta
    }

    /// Fold the latest raw ETA into the smoothed one.
    fn update_eta(&mut self, downloaded: u64, total: u64, dt: f64) {
        let Some(rate) = self.rate_bps() else {
            return;
        };
        if total == 0 || downloaded >= total {
            self.smoothed_eta = None;
            return;
        }

        // Byte counts are far below f64's exact-integer range.
        #[allow(clippy::cast_precision_loss)]
        let remaining = (total - downloaded) as f64;
        let raw = remaining / rate;
        if !raw.is_finite() {
            return;
        }

        self.smoothed_eta = Some(self.smoothed_eta.map_or(raw, |prev| {
            let alpha = 1.0 - (-dt / ETA_TAU).exp();
            alpha.mul_add(raw - prev, prev)
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Drive the estimator at `rate` bytes/sec for `steps` ticks of `dt`.
    fn drive(
        est: &mut RateEstimator,
        start: Instant,
        total: u64,
        rate: f64,
        dt: f64,
        steps: u32,
    ) -> Instant {
        let mut now = start;
        let mut bytes = est.prev_bytes.unwrap_or(0);
        for _ in 0..steps {
            now += Duration::from_secs_f64(dt);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let step = (rate * dt) as u64;
            bytes += step;
            est.record(bytes, total, now);
        }
        now
    }

    #[test]
    fn first_sample_never_reports_a_rate() {
        // A resumed download reports 2 GiB already on disk in its first event.
        let now = Instant::now();
        let mut est = RateEstimator::new(now);
        est.record(2 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024, now);
        assert_eq!(
            est.rate_bps(),
            None,
            "baseline sample must not yield a rate"
        );
    }

    #[test]
    fn converges_to_a_constant_rate() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, u64::MAX, start);
        drive(&mut est, start, u64::MAX, 100_000_000.0, 0.25, 240);

        let rate = est.rate_bps().expect("rate after 60s of steady transfer");
        let error = (rate - 100_000_000.0).abs() / 100_000_000.0;
        assert!(error < 0.02, "expected within 2% of 100 MB/s, got {rate}");
    }

    #[test]
    fn bursty_input_reads_as_steady() {
        // All bytes for a 2s window land in a single 250ms tick — the shape the
        // hf-xet stat poller actually produces when the chunk cache flushes.
        // The mean is 50 MB/s and the display must not swing around it.
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, u64::MAX, start);

        let mut now = start;
        let mut bytes = 0u64;
        let (mut low, mut high) = (f64::MAX, 0.0f64);

        for i in 0..320 {
            now += Duration::from_secs_f64(0.25);
            if i % 8 == 0 {
                bytes += 100_000_000; // 100 MB every 2s
            }
            est.record(bytes, u64::MAX, now);

            // Ignore the ramp; measure the steady state over the last 20s.
            if i >= 240 {
                if let Some(r) = est.rate_bps() {
                    low = low.min(r);
                    high = high.max(r);
                }
            }
        }

        for (label, value) in [("min", low), ("max", high)] {
            let error = (value - 50_000_000.0).abs() / 50_000_000.0;
            assert!(
                error < 0.15,
                "steady-state {label} was {value}, more than 15% off the 50 MB/s mean"
            );
        }
    }

    #[test]
    fn stall_decays_the_rate_toward_zero() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, u64::MAX, start);
        let mut now = drive(&mut est, start, u64::MAX, 100_000_000.0, 0.25, 120);
        let before = est.rate_bps().expect("rate before the stall");

        // Bytes stop moving; ticks keep arriving. This is the case the old
        // estimator got wrong — it only sampled when bytes changed, so a stall
        // froze the speed and left the ETA counting down against nothing.
        let stalled_at = est.prev_bytes.unwrap();
        for _ in 0..240 {
            now += Duration::from_secs_f64(0.25);
            est.record(stalled_at, u64::MAX, now);
        }

        let after = est.rate_bps().unwrap_or(0.0);
        assert!(
            after < before * 0.05,
            "60s stall should decay {before} to near zero, got {after}"
        );
    }

    #[test]
    fn shard_boundary_does_not_disturb_the_rate() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, u64::MAX, start);
        let now = drive(&mut est, start, u64::MAX, 100_000_000.0, 0.25, 160);
        let before = est.rate_bps().expect("rate at the end of shard 1");

        // Next shard: the per-shard counter restarts at zero.
        est.record(0, u64::MAX, now + Duration::from_secs_f64(0.25));
        let after = est.rate_bps().expect("rate immediately after the boundary");

        let change = (after - before).abs() / before;
        assert!(
            change < 0.10,
            "boundary changed the rate from {before} to {after}"
        );
    }

    #[test]
    fn eta_is_none_until_a_rate_exists() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, 1_000_000_000, start);
        est.record(1_000_000, 1_000_000_000, start + Duration::from_millis(250));
        assert_eq!(est.eta_seconds(), None, "no ETA before warmup");
    }

    #[test]
    fn eta_counts_down_on_a_steady_transfer() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        let total = 10_000_000_000u64; // 10 GB at 100 MB/s = 100s
        est.record(0, total, start);
        drive(&mut est, start, total, 100_000_000.0, 0.25, 240);

        let eta = est.eta_seconds().expect("ETA on a steady transfer");
        // 60s elapsed, 6 GB done, 4 GB left at 100 MB/s ≈ 40s.
        assert!(
            (eta - 40.0).abs() < 5.0,
            "expected ~40s remaining, got {eta}"
        );
    }

    #[test]
    fn eta_clears_on_completion() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        let total = 10_000_000_000u64;
        est.record(0, total, start);
        let now = drive(&mut est, start, total, 100_000_000.0, 0.25, 40);
        assert!(est.eta_seconds().is_some(), "ETA while in flight");

        est.record(total, total, now + Duration::from_millis(250));
        assert_eq!(est.eta_seconds(), None, "complete transfer has no ETA");
    }

    #[test]
    fn zero_total_yields_no_eta() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, 0, start);
        drive(&mut est, start, 0, 50_000_000.0, 0.25, 40);
        assert!(
            est.rate_bps().is_some(),
            "rate is known even without a total"
        );
        assert_eq!(est.eta_seconds(), None, "unknown total means no ETA");
    }
}
