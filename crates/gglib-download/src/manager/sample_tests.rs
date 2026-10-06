//! What the progress bridge's samples make of a download's counts.

use std::time::{Duration, Instant};

use crate::executor::FileProgress;
use crate::meter::Meter;

use super::*;

const MIB: u64 = 1024 * 1024;
const TICK: Duration = Duration::from_millis(250);

/// The accelerator as it was observed: the network steady, and the file
/// growing by 700 MiB every 13 seconds. The speed shown is the network's.
#[tokio::test]
async fn speed_from_wire_eta_from_bytes() {
    // 13 seconds are 52 ticks.
    let step = 700 * MIB;
    let per_tick = u32::try_from(step / 52).expect("a tick's bytes fit in a u32");
    let network_bps = f64::from(per_tick) * 4.0;

    let start = Instant::now();
    let meter = Arc::new(Mutex::new(Meter::new(start)));
    let mut progress = ProgressUpdate::default();
    let mut eta = None;
    for tick in 0..=480_u32 {
        let elapsed = f64::from(tick) * TICK.as_secs_f64();
        progress.progress = FileProgress {
            bytes: u64::from(tick / 52) * step,
            wire: u64::from(per_tick) * u64::from(tick),
            size: Some(100 * step),
        };

        let (speed, remaining) = sample(&meter, None, &progress, start + TICK * tick).await;

        // Past the warm-up, the speed is the network's at every tick.
        if elapsed >= 5.0 {
            let speed = speed.expect("a speed after the warm-up");
            let off = (speed - network_bps).abs() / network_bps;
            assert!(off < 0.05, "{off:.3} off at {elapsed}s: {speed} B/s");
        }
        eta = remaining;
    }

    // 9 of 100 steps are on disk after two minutes: about twenty minutes to
    // go. Counted from the bytes received it would be the same here, so the
    // meter's own tests tell the two apart.
    let eta = eta.expect("the size is known");
    assert!((600.0..2400.0).contains(&eta), "{eta}s");
}
