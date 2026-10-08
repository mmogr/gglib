//! Time-decayed download rate and ETA estimation.
//!
//! This is the single owner of all download speed / ETA math. The download
//! manager owns two [`RateEstimator`]s per shard group, one for the speed and
//! one for the time remaining, and ships the values they produce on the wire;
//! every renderer (CLI progress bars, Tauri GUI, web UI) displays those values
//! verbatim. Renderers must never re-derive a rate from byte deltas — doing so
//! is what let the CLI and the GUI disagree with each other, and with the
//! operating system's own network monitor.
//!
//! # Why not an exponentially weighted average of instantaneous rates?
//!
//! Progress arrives in bursts. On the `hf-xet` fast path the bytes on disk rise
//! in large steps, as the chunk cache flushes, even while the network rate is
//! perfectly flat. Dividing a burst by the short interval it landed in yields an
//! enormous instantaneous rate, and a running average of those still spikes.
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
//! Three further properties matter for how the manager drives this:
//!
//! * The first sample only establishes a baseline and yields no rate. A resumed
//!   download reports its whole on-disk size in the first event; counting that
//!   as bytes transferred "just now" is what produced multi-GB/s readings.
//! * Nothing is measured until the count first moves, and the sample that sees
//!   it move is a baseline too. A meter samples from before its connection
//!   delivers anything, and that wait is not time spent transferring: counted,
//!   it is what the first bytes are divided by. Nor do the first bytes seen say
//!   how fast the transfer runs. They may be a single read, or a whole step of
//!   a file written in steps, the bytes of many seconds landing in one tick.
//! * A byte count that moves backwards re-baselines instead of underflowing.
//!   The manager relies on it: the aggregate bytes on disk are not monotonic
//!   on the fallback path where shard sizes are unknown.
//!
//! # The time remaining
//!
//! What is left, divided by the rate smoothed once more. The second smoothing
//! is of the rate and not of the figure, because the rate is what holds still
//! on a steady transfer: the figure then counts down with the clock, and a
//! stall lengthens it from its first tick. An average of the figure itself
//! runs late by its own time constant, and nothing bounds what goes into it:
//! one enormous early value, from a rate near zero, is still in it half a
//! minute on.

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

/// Time constant for the second smoothing of the rate, which the time
/// remaining divides by.
///
/// The ripple [`RATE_TAU`] leaves reads as steady in a speed and not in a
/// countdown: 7% either way moves a ten-minute figure by eighty seconds,
/// gained between two bursts and lost at the next. Shorter than [`RATE_TAU`],
/// so a real change in throughput reaches the figure almost as soon as it
/// reaches the rate.
const ETA_TAU: f64 = 5.0;

/// Minimum time measured before any rate is reported.
///
/// Below this the average is dominated by whatever the first interval happened
/// to contain, so [`RateEstimator::rate_bps`] reports `None` and callers render
/// a placeholder instead of a number that is about to change by an order of
/// magnitude. It is time in which the transfer was under way: the wait before
/// the count first moves is not measured, so it cannot stand in for the
/// warm-up and leave the first bytes to be reported the moment they arrive.
const WARMUP_SECS: f64 = 1.5;

/// Time-decayed estimate of download throughput and time remaining.
///
/// Feed it cumulative byte counts with [`record`](Self::record) — on *every*
/// tick, including ticks where the count has not changed, since those are what
/// make a stalled transfer decay toward zero. The ticks before the count first
/// moves are the exception: they are the wait for the transfer to begin.
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
    /// The count has moved since the first sample: the transfer is under way,
    /// and every sample from here on is measured.
    started: bool,
    /// The rate smoothed once more, for the time remaining to divide by;
    /// `None` until there is a rate.
    eta_rate: Option<f64>,
    /// Seconds remaining; `None` when unknown or complete.
    eta: Option<f64>,
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
            started: false,
            eta_rate: None,
            eta: None,
        }
    }

    /// Record a cumulative byte count.
    ///
    /// `downloaded` and `total` are cumulative totals for the whole artifact,
    /// not per-tick deltas. `total` may be `0` when the size is not yet known,
    /// in which case no ETA is produced.
    ///
    /// Call this on every tick of the download's meter. Ticks where `downloaded`
    /// has not moved are meaningful samples: they are how a stall pulls the
    /// reported rate down, once the count has moved at all.
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
            // Counter moved backwards (per-file counters restart at zero, and
            // the unknown-shard-size fallback is not monotonic). Re-baseline
            // without emitting a sample, keeping the accumulated average so the
            // user sees no discontinuity at a shard boundary.
            self.prev_bytes = Some(downloaded);
            return;
        }

        self.prev_bytes = Some(downloaded);

        if !self.started {
            // The transfer has not begun, or this is the sample that sees it
            // begin. Either way it is a baseline: the wait for a connection is
            // not transfer time, and the first bytes seen are not the work of
            // the tick that caught them. They are one read at its very end, or
            // a step that took many seconds to fill.
            self.started = downloaded > prev;
            return;
        }

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

    /// The seconds remaining: what is left at the rate, smoothed once more.
    ///
    /// `None` when the total size is unknown, the transfer is complete, or no
    /// rate has been established yet.
    #[must_use]
    pub const fn eta_seconds(&self) -> Option<f64> {
        self.eta
    }

    /// Fold the latest rate into the smoothed one, and divide what is left by
    /// it.
    fn update_eta(&mut self, downloaded: u64, total: u64, dt: f64) {
        let Some(rate) = self.rate_bps() else {
            return;
        };
        let rate = self.eta_rate.map_or(rate, |prev| {
            let alpha = 1.0 - (-dt / ETA_TAU).exp();
            alpha.mul_add(rate - prev, prev)
        });
        self.eta_rate = Some(rate);

        if total == 0 || downloaded >= total {
            self.eta = None;
            return;
        }

        // Byte counts are far below f64's exact-integer range.
        #[allow(clippy::cast_precision_loss)]
        let remaining = (total - downloaded) as f64;
        let eta = remaining / rate;
        if eta.is_finite() {
            self.eta = Some(eta);
        }
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
        // All bytes for a 2s window land in a single 250ms tick — the shape of
        // hf-xet's bytes on disk when the chunk cache flushes.
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

    /// A tick of a download's meter.
    const TICK: Duration = Duration::from_millis(250);
    /// What a tick carries at 10 MB/s.
    const PER_TICK: u32 = 2_500_000;
    /// All that the tick which first sees bytes catches of them.
    const FIRST_READ: u64 = 64 * 1024;

    /// A download as its meter feeds it, up to its first bytes: two seconds
    /// of ticks while the connection is made, then the first read. `ticks`
    /// more at 10 MB/s complete it. Returns the estimator, the clock, the
    /// bytes so far and the size.
    fn connected(ticks: u32) -> (RateEstimator, Instant, u64, u64) {
        let total = FIRST_READ + u64::from(ticks) * u64::from(PER_TICK);
        let mut now = Instant::now();
        let mut est = RateEstimator::new(now);
        est.record(0, total, now);
        for _ in 0..8 {
            now += TICK;
            est.record(0, total, now);
        }
        now += TICK;
        est.record(FIRST_READ, total, now);
        (est, now, FIRST_READ, total)
    }

    #[test]
    fn the_time_remaining_is_right_from_the_first_figure_shown() {
        // 300 MB at 10 MB/s: thirty seconds. Every figure shown is within a
        // quarter of what is left at that rate, and there is one to show five
        // seconds after the first byte.
        let ticks = 120;
        let (mut est, mut now, mut bytes, total) = connected(ticks);

        for tick in 1..ticks {
            now += TICK;
            bytes += u64::from(PER_TICK);
            est.record(bytes, total, now);

            let since_first_byte = f64::from(tick) * 0.25;
            let truth = f64::from(ticks - tick) * 0.25;
            let Some(eta) = est.eta_seconds() else {
                assert!(
                    since_first_byte < 5.0,
                    "no time remaining {since_first_byte}s after the first byte"
                );
                continue;
            };
            assert!(
                (eta - truth).abs() <= truth * 0.25,
                "{eta:.1}s shown with {truth:.1}s left, {since_first_byte}s after the first byte"
            );
        }
    }

    #[test]
    fn nothing_is_reported_in_the_first_second_of_a_transfer() {
        let (mut est, mut now, mut bytes, total) = connected(120);

        for _ in 0..4 {
            now += TICK;
            bytes += u64::from(PER_TICK);
            est.record(bytes, total, now);
            assert_eq!(
                (est.rate_bps(), est.eta_seconds()),
                (None, None),
                "the wait for the connection is not time spent measuring"
            );
        }
    }

    #[test]
    fn the_time_remaining_counts_down_with_the_clock() {
        // Five minutes of transfer, watched for the first of them.
        let (mut est, mut now, mut bytes, total) = connected(1200);

        let mut last = None;
        let mut compared = 0;
        for _ in 0..240 {
            now += TICK;
            bytes += u64::from(PER_TICK);
            est.record(bytes, total, now);

            let eta = est.eta_seconds();
            if let (Some(before), Some(after)) = (last, eta) {
                let fell: f64 = before - after;
                assert!(
                    (fell - 0.25).abs() < 0.025,
                    "{before:.2}s then {after:.2}s, a quarter of a second apart"
                );
                compared += 1;
            }
            last = eta;
        }
        assert!(compared > 200, "only {compared} figures to compare");
    }

    #[test]
    fn bursty_input_does_not_swing_the_time_remaining() {
        // The shape of `bursty_input_reads_as_steady`: 100 MB every 2s, a
        // mean of 50 MB/s, toward 60 GB.
        let total_mb = 60_000_u32;
        let total = u64::from(total_mb) * 1_000_000;
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, total, start);

        let mut now = start;
        let mut bursts = 0_u32;
        for i in 0..480 {
            now += TICK;
            if i % 8 == 0 {
                bursts += 1;
            }
            est.record(u64::from(bursts) * 100_000_000, total, now);

            // Past the ramp, every figure is what is left at the mean rate.
            if i >= 240 {
                let eta = est.eta_seconds().expect("a time remaining after a minute");
                let truth = f64::from(total_mb - bursts * 100) / 50.0;
                assert!(
                    (eta - truth).abs() < truth * 0.02,
                    "{eta:.0}s shown with {truth:.0}s left at the mean rate"
                );
            }
        }
    }

    #[test]
    fn a_stall_never_shortens_the_time_remaining() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        let total = 10_000_000_000u64;
        est.record(0, total, start);
        let mut now = drive(&mut est, start, total, 100_000_000.0, 0.25, 120);
        let before = est
            .eta_seconds()
            .expect("a time remaining before the stall");

        let stalled_at = est.prev_bytes.unwrap();
        let mut last = before;
        for _ in 0..240 {
            now += TICK;
            est.record(stalled_at, total, now);
            let eta = est.eta_seconds().expect("a time remaining in the stall");
            assert!(
                eta > last,
                "{last:.2}s then {eta:.2}s with nothing arriving"
            );
            last = eta;
        }
        assert!(
            last > before * 10.0,
            "60s of nothing moved {before:.0}s to only {last:.0}s"
        );
    }

    #[test]
    fn a_shard_boundary_does_not_move_the_time_remaining() {
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        let total = 10_000_000_000u64;
        est.record(0, total, start);
        let now = drive(&mut est, start, total, 100_000_000.0, 0.25, 160);
        let before = est.eta_seconds().expect("a time remaining in shard 1");

        // Next shard: the per-shard counter restarts at zero, and its size is
        // what the shards before it left to fetch.
        let left = total - est.prev_bytes.unwrap();
        est.record(0, left, now + TICK);
        assert_eq!(est.eta_seconds(), Some(before), "at the boundary");

        // The countdown goes on from there: a tick later, a tick less.
        est.record(25_000_000, left, now + TICK * 2);
        let after = est.eta_seconds().expect("a time remaining in shard 2");
        assert!(
            (before - after - 0.25).abs() < 0.05,
            "{before:.2}s at the boundary and {after:.2}s a tick after it"
        );
    }

    #[test]
    fn a_file_written_in_steps_is_not_timed_by_its_first_step() {
        // The accelerator as the meter sees it: 700 MiB on disk every 13
        // seconds, which is 52 ticks, toward a hundred such steps.
        let step = 700 * 1024 * 1024_u64;
        let start = Instant::now();
        let mut est = RateEstimator::new(start);
        est.record(0, 100 * step, start);

        let mut now = start;
        let mut steps = 0_u32;
        let mut first = None;
        for tick in 1..=120_u32 {
            now += TICK;
            if tick % 52 == 0 {
                steps += 1;
            }
            est.record(u64::from(steps) * step, 100 * step, now);
            first = first.or_else(|| est.eta_seconds().map(|eta| (eta, 100 - steps)));
        }

        // A step is the bytes of the thirteen seconds before it. One step
        // alone is not a rate: taken over the tick it landed in, it would
        // promise the file in a seventh of the time.
        let (eta, steps_left) = first.expect("a time remaining within thirty seconds");
        let truth = f64::from(steps_left) * 13.0;
        assert!(eta > truth * 0.5, "{eta:.0}s shown with {truth:.0}s left");
    }
}
