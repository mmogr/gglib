//! Speed and time remaining for one download.
//!
//! Two questions with two different inputs. How fast the download is going
//! is a question about the network, so the speed is taken from the bytes
//! received. How long it has left is a question about the file, so the time
//! remaining is taken from the bytes on disk and the size.
//!
//! The bytes on disk are no measure of speed. The accelerator writes them in
//! steps of hundreds of megabytes while the network runs flat, a resume
//! starts with a file's worth of them already there, and a file found on
//! disk arrives all at once.
//!
//! Nor did the bytes found on disk take any time to arrive. The time
//! remaining leaves them out of its rate, and counts only what is left.

use std::time::Instant;

use gglib_core::download::RateEstimator;

/// The two estimates of one download, each fed its own count.
#[derive(Debug, Clone)]
pub(crate) struct Meter {
    speed: RateEstimator,
    remaining: RateEstimator,
    /// Bytes on disk beyond the bytes received, by the two counts it is
    /// given: what a resume found there, and a file that was already
    /// complete.
    found: u64,
}

impl Meter {
    /// A meter with its baseline at `now`.
    pub(crate) const fn new(now: Instant) -> Self {
        Self {
            speed: RateEstimator::new(now),
            remaining: RateEstimator::new(now),
            found: 0,
        }
    }

    /// Take one sample, on every tick, moved or not.
    ///
    /// `wire` is the bytes received. A count that starts again, as one kept
    /// per file does at each file of a download, is taken by the estimator
    /// as a new baseline. `bytes` of `total` are the download's bytes on
    /// disk, with a `total` of 0 when the size is not known.
    pub(crate) fn record(&mut self, wire: u64, bytes: u64, total: u64, now: Instant) {
        self.speed.record(wire, 0, now);

        // Whatever is on disk beyond what the file has received was found
        // there. It comes off both ends of the count, so the bytes left stay
        // `total - bytes` while the rate sees only bytes that took time. The
        // `min` is for a count that falls: a restart cannot have found more
        // than is there now.
        self.found = self.found.max(bytes.saturating_sub(wire)).min(bytes);
        self.remaining
            .record(bytes - self.found, total.saturating_sub(self.found), now);
    }

    /// Bytes per second off the network, once there is enough to go on.
    pub(crate) fn speed_bps(&self) -> Option<f64> {
        self.speed.rate_bps()
    }

    /// Seconds until the bytes on disk reach the size, when that is known.
    pub(crate) const fn eta_seconds(&self) -> Option<f64> {
        self.remaining.eta_seconds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;
    const TICK: Duration = Duration::from_millis(250);
    /// What the network delivers in one tick: 20 MiB/s.
    const PER_TICK: u64 = 5 * MIB;

    /// Sample a file that had `on_disk` bytes when its transfer began, for
    /// `ticks` ticks from `from`, on top of `before` bytes of earlier files.
    /// Returns the bytes on disk at the last sample.
    fn transfer(
        meter: &mut Meter,
        start: Instant,
        from: u32,
        ticks: u32,
        (before, on_disk, total): (u64, u64, u64),
    ) -> u64 {
        let mut bytes = before + on_disk;
        for tick in 0..ticks {
            let wire = PER_TICK * u64::from(tick);
            bytes = before + on_disk + wire;
            meter.record(wire, bytes, total, start + TICK * (from + tick));
        }
        bytes
    }

    /// The time remaining, against what is left at 20 MiB/s. The smoothed
    /// figure trails the true one, and the ticks before the first byte count
    /// as time in which nothing arrived.
    fn assert_time_remaining(meter: &Meter, bytes: u64, total: u64) {
        let eta = meter.eta_seconds().expect("a time remaining");
        #[allow(clippy::cast_precision_loss)]
        let truth = (total - bytes) as f64 / (4 * PER_TICK) as f64;
        let off = (eta - truth).abs() / truth;
        assert!(off < 0.1, "{eta:.0}s shown with {truth:.0}s left");
    }

    /// A resume: half the file is on disk before a byte is received. The
    /// first sample is of nothing, as the meter task's is, so the bytes
    /// on disk turn up between two samples. What is left to wait for is what
    /// is missing from the disk, at the rate the rest is arriving.
    #[test]
    fn the_time_remaining_counts_the_bytes_on_disk() {
        let total = 40 * GIB;
        let start = Instant::now();
        let mut meter = Meter::new(start);
        meter.record(0, 0, 0, start);

        let bytes = transfer(&mut meter, start, 1, 60, (0, 20 * GIB, total));

        assert_time_remaining(&meter, bytes, total);
    }

    /// The first file of a download was already there, and the second is
    /// fetched.
    #[test]
    fn a_file_found_on_disk_does_not_shorten_the_time_remaining() {
        let (first, total) = (8 * GIB, 40 * GIB);
        let start = Instant::now();
        let mut meter = Meter::new(start);
        meter.record(0, 0, 0, start);
        meter.record(0, first, total, start + TICK);

        let bytes = transfer(&mut meter, start, 2, 60, (first, 0, total));

        assert_time_remaining(&meter, bytes, total);
    }

    /// The accelerator fails and the native transport starts the file again
    /// with less on disk: the count falls, and what was found falls with it.
    #[test]
    fn a_restart_keeps_the_time_remaining_on_what_is_left() {
        let total = 40 * GIB;
        let start = Instant::now();
        let mut meter = Meter::new(start);
        meter.record(0, 0, 0, start);
        transfer(&mut meter, start, 1, 60, (0, 20 * GIB, total));

        let bytes = transfer(&mut meter, start, 61, 120, (0, GIB, total));

        assert_time_remaining(&meter, bytes, total);
    }

    /// A file found on disk is bytes on disk and nothing received.
    #[test]
    fn a_file_found_on_disk_is_no_speed() {
        let start = Instant::now();
        let mut meter = Meter::new(start);
        for tick in 0..=12_u32 {
            let found = if tick >= 4 { 4096 * MIB } else { 0 };
            meter.record(0, found, 8192 * MIB, start + TICK * tick);
        }

        assert_eq!(meter.speed_bps(), None);
    }
}
