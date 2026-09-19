//! A scan recorded while the loop guard's writer stops is either written or
//! counted as lost, never neither (#1098).
//!
//! `LoopGuardTripWriter::record_scan` checks whether the writer has stopped
//! under the lock the stop's last flush takes the counts with. With the check
//! outside that lock, a scan could pass the check before the stop and land in
//! the counts after that flush took them: written nowhere, and counted
//! nowhere. No other test reaches that window. This one races three recording
//! threads against `shutdown()`, and checks every round's accounts.
//!
//! With the check where it is, the assertion holds whatever the scheduling.
//! With it moved outside the lock, a reviewer's experiment for #1095 lost a
//! scan in 171 of 200 rounds; at that rate, twenty rounds all missing it would
//! be chance of under one in 10^16. Moving the check out is this test's
//! positive control.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gglib_core::LoopGuardMode;
use gglib_core::ports::LoopGuardTripSink;
use gglib_db::{LoopGuardTripWriter, setup_database};

const ROUNDS: usize = 20;
const RECORDERS: usize = 3;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// One round: recorders run until just after the stop, then the scans the
/// threads counted are compared with the scans written plus those lost.
async fn one_round() -> (i64, i64, i64) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = setup_database(&dir.path().join("gglib.db"))
        .await
        .expect("the database");
    let writer = LoopGuardTripWriter::spawn(pool.clone());
    let stop = Arc::new(AtomicBool::new(false));
    let recorded = Arc::new(AtomicU64::new(0));
    let recorders: Vec<_> = (0..RECORDERS)
        .map(|_| {
            let (writer, stop, recorded) = (
                Arc::clone(&writer),
                Arc::clone(&stop),
                Arc::clone(&recorded),
            );
            std::thread::spawn(move || {
                let at = now_secs();
                while !stop.load(Ordering::Relaxed) {
                    writer.record_scan("qwen", LoopGuardMode::Note, at);
                    recorded.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(2)).await;
    writer.shutdown().await;
    tokio::time::sleep(Duration::from_millis(1)).await;
    stop.store(true, Ordering::Relaxed);
    for recorder in recorders {
        recorder.join().expect("a recording thread");
    }
    let written: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(scanned), 0) FROM loop_guard_scans")
        .fetch_one(&pool)
        .await
        .expect("the scans written");
    let lost = i64::try_from(writer.lost()).expect("the lost count fits");
    let recorded = i64::try_from(recorded.load(Ordering::Relaxed)).expect("the count fits");
    (recorded, written, lost)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_scan_racing_the_stop_is_written_or_counted_as_lost() {
    let mut unaccounted = Vec::new();
    for round in 0..ROUNDS {
        let (recorded, written, lost) = one_round().await;
        if recorded != written + lost {
            unaccounted.push(format!(
                "round {round}: recorded {recorded}, written {written}, lost {lost}"
            ));
        }
    }
    assert!(
        unaccounted.is_empty(),
        "{} of {ROUNDS} rounds lost a scan, neither written nor counted: {unaccounted:?}",
        unaccounted.len()
    );
}
