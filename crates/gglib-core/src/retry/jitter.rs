//! Jitter source for [`decide`](super::decide).
//!
//! Kept separate from the policy so the policy itself stays a pure function of
//! its arguments — tests inject an exact `jitter_unit` and never touch this.

use std::time::{SystemTime, UNIX_EPOCH};

/// A value in `[0.0, 1.0)` for use as `jitter_unit`.
///
/// The bar here is "decorrelates concurrent clients", not statistical
/// uniformity: two processes would have to back off within the same nanosecond
/// to collide. That is met without a dependency, which matters because the
/// workspace already carries several `rand` versions and neither consumer of
/// this needs a third.
///
/// [`decide`](super::decide) clamps whatever it receives, so a coarse clock on
/// some platform can degrade the spread but cannot produce an invalid delay.
#[must_use]
pub fn jitter_unit() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    f64::from(nanos) / 1_000_000_000.0
}
