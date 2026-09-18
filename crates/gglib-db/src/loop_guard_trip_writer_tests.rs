//! Tests for [`super`]: that recording never waits, that stopping writes what
//! was recorded, that the timer flushes a day's scans on its own, and that a
//! refused flush does not end the task.

use std::time::Duration;

use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::domain::loop_guard_log::{LoopGuardTripDay, epoch_day};
use gglib_core::ports::LoopGuardTripLog;

use super::*;
use crate::repositories::SqliteLoopGuardTripLog;
use crate::setup::{setup_database, setup_test_database};

fn trip(model: &str) -> LoopGuardTripEvent {
    LoopGuardTripEvent::new(now_secs(), model, LoopGuardTrip::Loop, LoopGuardMode::Note)
}

async fn today(pool: &SqlitePool) -> Vec<LoopGuardTripDay> {
    SqliteLoopGuardTripLog::new(pool.clone())
        .summary(epoch_day(now_secs()))
        .await
        .unwrap()
}

/// A writer whose queue nothing drains, so a test can fill it.
fn undrained(capacity: usize) -> (LoopGuardTripWriter, mpsc::Receiver<LoopGuardTripEvent>) {
    let (trips, queue) = mpsc::channel(capacity);
    let writer = LoopGuardTripWriter {
        trips,
        shared: Arc::new(Shared::default()),
        stop: Mutex::new(None),
        task: Mutex::new(None),
    };
    (writer, queue)
}

#[test]
fn a_full_queue_drops_and_counts_rather_than_waiting() {
    let (writer, _queue) = undrained(2);
    let writer = Arc::new(writer);
    let (done, finished) = std::sync::mpsc::channel();
    let recorder = Arc::clone(&writer);
    // A plain thread, not a runtime: a recorder that blocked would block here
    // for good, and the timeout below is what notices — not a panic.
    std::thread::spawn(move || {
        for _ in 0..3 {
            recorder.record_trip(trip("m"));
        }
        done.send(()).unwrap();
    });

    finished
        .recv_timeout(Duration::from_secs(2))
        .expect("recording onto a full queue must return at once");
    assert_eq!(
        writer.lost(),
        1,
        "two fit, the third is dropped and counted"
    );
}

#[tokio::test]
async fn stopping_writes_every_trip_and_every_scan_recorded_before_it() {
    let pool = setup_test_database().await.unwrap();
    let writer = LoopGuardTripWriter::spawn(pool.clone());
    let at = now_secs();
    for _ in 0..3 {
        writer.record_scan("qwen", LoopGuardMode::Note, at);
    }
    writer.record_trip(trip("qwen"));
    writer.record_trip(trip("qwen"));

    // The timer's first tick is five seconds away, so nothing but the stop
    // can have written these.
    writer.shutdown().await;

    let days = today(&pool).await;
    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!((days[0].scanned, days[0].trips), (3, 2));
    assert_eq!(writer.lost(), 0);
    writer.shutdown().await;
}

#[tokio::test]
async fn a_trip_outlives_the_database_connection_that_wrote_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gglib.db");
    let pool = setup_database(&path).await.unwrap();
    let writer = LoopGuardTripWriter::spawn(pool.clone());
    writer.record_scan("qwen", LoopGuardMode::Refuse, now_secs());
    writer.record_trip(
        LoopGuardTripEvent::new(
            now_secs(),
            "qwen",
            LoopGuardTrip::Loop,
            LoopGuardMode::Refuse,
        )
        .with_signature("write_file:1"),
    );
    writer.shutdown().await;
    pool.close().await;

    let reopened = setup_database(&path).await.unwrap();
    let days = today(&reopened).await;
    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!(days[0].mode, LoopGuardMode::Refuse);
    assert_eq!((days[0].scanned, days[0].trips, days[0].loops), (1, 1, 1));
}

#[tokio::test]
async fn the_timer_flushes_scans_with_no_trip_among_them() {
    let pool = setup_test_database().await.unwrap();
    let writer = LoopGuardTripWriter::spawn_with(
        pool.clone(),
        TripWriterLimits {
            period: Duration::from_millis(50),
            ..TripWriterLimits::default()
        },
    );
    writer.record_scan("qwen", LoopGuardMode::Note, now_secs());

    let mut seen = Vec::new();
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        seen = today(&pool).await;
        if !seen.is_empty() {
            break;
        }
    }
    assert_eq!(seen.len(), 1, "the timer never flushed the scan");
    assert_eq!((seen[0].scanned, seen[0].trips), (1, 0));
    writer.shutdown().await;
}

#[tokio::test]
async fn a_refused_flush_loses_its_batch_and_the_writer_carries_on() {
    let pool = setup_test_database().await.unwrap();
    let writer = LoopGuardTripWriter::spawn_with(
        pool.clone(),
        TripWriterLimits {
            period: Duration::from_millis(50),
            ..TripWriterLimits::default()
        },
    );
    // Let the pruning at start finish before the pool goes away.
    tokio::time::sleep(Duration::from_millis(20)).await;
    pool.close().await;
    writer.record_trip(trip("qwen"));
    writer.record_scan("qwen", LoopGuardMode::Note, now_secs());
    writer.record_scan("qwen", LoopGuardMode::Note, now_secs());

    for _ in 0..40 {
        if writer.lost() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        writer.lost(),
        3,
        "the refused batch is counted: one trip, two scans"
    );
    assert!(
        !writer.trips.is_closed(),
        "a refused flush must not end the task that drains the queue"
    );
    writer.shutdown().await;
}

#[tokio::test]
async fn what_is_recorded_after_stopping_is_counted_as_lost_not_kept() {
    let pool = setup_test_database().await.unwrap();
    let writer = LoopGuardTripWriter::spawn(pool.clone());
    writer.shutdown().await;

    writer.record_scan("qwen", LoopGuardMode::Note, now_secs());
    writer.record_trip(trip("qwen"));

    assert_eq!(writer.lost(), 2, "one scan and one decision, both counted");
    assert!(
        lock(&writer.shared.scans).is_empty(),
        "nothing will flush a scan kept after the stop"
    );
}

async fn trip_rows(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM loop_guard_trips")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_flushed_batch_is_not_written_again() {
    let pool = setup_test_database().await.unwrap();
    let limits = TripWriterLimits {
        period: Duration::from_millis(50),
        ..TripWriterLimits::default()
    };
    let writer = LoopGuardTripWriter::spawn_with(pool.clone(), limits);
    for expected in 1..=2 {
        writer.record_trip(trip("qwen"));
        for _ in 0..40 {
            if trip_rows(&pool).await >= expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    writer.shutdown().await;
    assert_eq!(trip_rows(&pool).await, 2, "two timer flushes, two rows");
}

#[tokio::test]
async fn a_long_model_name_keys_its_scan_and_its_trip_alike() {
    let pool = setup_test_database().await.unwrap();
    let writer = LoopGuardTripWriter::spawn(pool.clone());
    let name = "m".repeat(300);
    writer.record_scan(&name, LoopGuardMode::Note, now_secs());
    writer.record_trip(trip(&name));
    writer.shutdown().await;

    let days = today(&pool).await;
    assert_eq!(
        days.len(),
        1,
        "one row, not a scan and a trip apart: {days:?}"
    );
    assert_eq!((days[0].scanned, days[0].trips), (1, 1));
    assert_eq!(days[0].model_name.chars().count(), 256);
}

#[tokio::test]
async fn a_new_writer_prunes_what_the_log_no_longer_keeps() {
    let pool = setup_test_database().await.unwrap();
    let old = now_secs() - 100 * 86_400;
    let mut conn = pool.acquire().await.unwrap();
    crate::repositories::sqlite_loop_guard_trip_log::write_batch(
        &mut conn,
        "0.0.0-test",
        &[LoopGuardTripEvent::new(
            old,
            "m",
            LoopGuardTrip::Loop,
            LoopGuardMode::Note,
        )],
        &HashMap::new(),
    )
    .await
    .unwrap();
    drop(conn);

    let writer = LoopGuardTripWriter::spawn(pool.clone());
    for _ in 0..40 {
        if trip_rows(&pool).await == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        trip_rows(&pool).await,
        0,
        "a row 100 days old is gone at start"
    );
    writer.shutdown().await;
}

#[tokio::test]
async fn a_batch_of_zero_is_taken_as_one() {
    let pool = setup_test_database().await.unwrap();
    let limits = TripWriterLimits {
        batch: 0,
        ..TripWriterLimits::default()
    };
    let writer = LoopGuardTripWriter::spawn_with(pool.clone(), limits);
    // Give the task time to prune at start and reach its loop. Recording and
    // stopping at once would let the stop arm drain the queue before the loop
    // ever asked it for a batch of zero, and prove nothing.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!writer.trips.is_closed(), "the writer stopped by itself");
    writer.record_trip(trip("qwen"));
    writer.shutdown().await;
    assert_eq!(trip_rows(&pool).await, 1);
    assert_eq!(writer.lost(), 0);
}

#[tokio::test]
async fn a_capacity_of_zero_is_taken_as_one() {
    let pool = setup_test_database().await.unwrap();
    let limits = TripWriterLimits {
        capacity: 0,
        ..TripWriterLimits::default()
    };
    let writer = LoopGuardTripWriter::spawn_with(pool.clone(), limits);
    writer.record_trip(trip("qwen"));
    writer.shutdown().await;
    assert_eq!(trip_rows(&pool).await, 1);
}
