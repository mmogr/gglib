//! Tests for [`super`]: the log read from a database with no daemon anywhere,
//! and what the table and the empty case say.

use gglib_core::LoopGuardMode;
use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::domain::loop_guard_log::LoopGuardTripEvent;
use gglib_core::ports::LoopGuardTripSink;
use gglib_db::setup::setup_test_database;
use gglib_db::{LoopGuardTripWriter, SqliteLoopGuardTripLog};

use super::*;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn printed(days: &[LoopGuardTripDay], since_days: u32) -> String {
    let mut out = Vec::new();
    render(&mut out, days, since_days).unwrap();
    String::from_utf8(out).unwrap()
}

#[tokio::test]
async fn the_log_is_read_from_the_database_with_no_daemon_involved() {
    let pool = setup_test_database().await.unwrap();
    let writer = LoopGuardTripWriter::spawn(pool.clone());
    let now = now_secs();
    for _ in 0..4 {
        writer.record_scan("qwen3-coder", LoopGuardMode::Note, now);
    }
    // Ten days back: outside the seven-day window read below.
    writer.record_scan("qwen3-coder", LoopGuardMode::Note, now - 10 * 86_400);
    writer.record_trip(
        LoopGuardTripEvent::new(
            now,
            "qwen3-coder",
            LoopGuardTrip::Stagnation,
            LoopGuardMode::Note,
        )
        .with_repeats(6, 5),
    );
    writer.shutdown().await;

    let days = read(&SqliteLoopGuardTripLog::new(pool), 7, now)
        .await
        .unwrap();

    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!(
        (days[0].scanned, days[0].trips, days[0].stagnations),
        (4, 1, 1)
    );
    let text = printed(&days, 7);
    assert!(text.contains("qwen3-coder"), "{text}");
    assert!(
        text.starts_with("The loop guard's log, the last 7 day(s):\n"),
        "{text}"
    );
    let row = text.lines().nth(2).expect("the window, a header and a row");
    let cells: Vec<&str> = row.split_whitespace().collect();
    assert_eq!(cells[3..], ["note", "4", "1", "0", "1", "0"], "{row}");
}

#[test]
fn an_empty_window_says_so_and_names_the_window() {
    let text = printed(&[], 7);
    assert!(text.contains("the last 7 day(s)"), "{text}");
    assert!(
        printed(&[], 10_000).contains("the last 90 day(s)"),
        "a window wider than the log keeps is shown as the one it reads"
    );
}
