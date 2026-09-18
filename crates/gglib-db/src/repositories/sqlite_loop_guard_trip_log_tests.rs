//! Tests for [`super`]: what a flush writes, what the summary reads back, and
//! what pruning keeps — straight against the statements, with no writer task
//! in the way.

use gglib_core::domain::loop_guard_log::first_day_of_window;

use super::*;
use crate::setup::setup_test_database;

/// 2026-09-18T12:00:00Z: epoch day 20714.
const NOON: u64 = 1_789_732_800;
const DAY: u64 = 86_400;
const TODAY: i64 = 20_714;

fn trip(at: u64, model: &str, detector: LoopGuardTrip, mode: LoopGuardMode) -> LoopGuardTripEvent {
    LoopGuardTripEvent::new(at, model, detector, mode)
}

fn scans(entries: &[(i64, &str, LoopGuardMode, u64)]) -> HashMap<ScanKey, u64> {
    entries
        .iter()
        .map(|(day, model, mode, n)| ((*day, (*model).to_owned(), *mode), *n))
        .collect()
}

async fn write(pool: &SqlitePool, trips: &[LoopGuardTripEvent], counted: &HashMap<ScanKey, u64>) {
    let mut conn = pool.acquire().await.unwrap();
    write_batch(&mut conn, "0.0.0-test", trips, counted)
        .await
        .unwrap();
}

async fn read(pool: &SqlitePool, first_day: i64) -> Vec<LoopGuardTripDay> {
    SqliteLoopGuardTripLog::new(pool.clone())
        .summary(first_day)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_day_scanned_without_a_trip_is_read_back_with_zero_trips() {
    let pool = setup_test_database().await.unwrap();
    write(
        &pool,
        &[],
        &scans(&[(TODAY, "qwen", LoopGuardMode::Note, 40)]),
    )
    .await;

    let days = read(&pool, TODAY).await;

    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!(days[0].day, "2026-09-18");
    assert_eq!(days[0].scanned, 40);
    assert_eq!(days[0].trips, 0);
}

#[tokio::test]
async fn the_summary_separates_the_detectors_and_counts_sessions() {
    let pool = setup_test_database().await.unwrap();
    let trips = [
        trip(NOON, "qwen", LoopGuardTrip::Loop, LoopGuardMode::Note)
            .with_signature("write_file:1")
            .with_session("a"),
        trip(NOON + 1, "qwen", LoopGuardTrip::Loop, LoopGuardMode::Note).with_session("a"),
        trip(
            NOON + 2,
            "qwen",
            LoopGuardTrip::Stagnation,
            LoopGuardMode::Note,
        )
        .with_repeats(6, 5)
        .with_session("b"),
    ];
    write(
        &pool,
        &trips,
        &scans(&[(TODAY, "qwen", LoopGuardMode::Note, 10)]),
    )
    .await;

    let days = read(&pool, TODAY).await;

    assert_eq!(days.len(), 1, "{days:?}");
    let d = &days[0];
    assert_eq!((d.scanned, d.trips), (10, 3));
    assert_eq!((d.loops, d.stagnations), (2, 1));
    assert_eq!(d.sessions, 2, "two distinct sessions tripped");
    assert_eq!(d.mode, LoopGuardMode::Note);
    assert_eq!(d.gglib_version, "0.0.0-test");
}

#[tokio::test]
async fn modes_and_models_are_read_apart() {
    let pool = setup_test_database().await.unwrap();
    write(
        &pool,
        &[trip(
            NOON,
            "qwen",
            LoopGuardTrip::Loop,
            LoopGuardMode::Refuse,
        )],
        &scans(&[
            (TODAY, "qwen", LoopGuardMode::Refuse, 3),
            (TODAY, "qwen", LoopGuardMode::Note, 5),
            (TODAY, "gemma", LoopGuardMode::Note, 7),
        ]),
    )
    .await;

    let days = read(&pool, TODAY).await;

    let got: Vec<_> = days
        .iter()
        .map(|d| (d.model_name.as_str(), d.mode, d.scanned, d.trips))
        .collect();
    assert_eq!(
        got,
        vec![
            ("gemma", LoopGuardMode::Note, 7, 0),
            ("qwen", LoopGuardMode::Note, 5, 0),
            ("qwen", LoopGuardMode::Refuse, 3, 1),
        ]
    );
}

#[tokio::test]
async fn a_second_flush_adds_to_the_day_it_scanned() {
    let pool = setup_test_database().await.unwrap();
    let counted = scans(&[(TODAY, "qwen", LoopGuardMode::Note, 4)]);
    write(&pool, &[], &counted).await;
    write(&pool, &[], &counted).await;

    assert_eq!(read(&pool, TODAY).await[0].scanned, 8);
}

#[tokio::test]
async fn the_window_starts_on_its_first_day() {
    let pool = setup_test_database().await.unwrap();
    write(
        &pool,
        &[trip(
            NOON - DAY,
            "qwen",
            LoopGuardTrip::Loop,
            LoopGuardMode::Note,
        )],
        &scans(&[
            (TODAY - 1, "qwen", LoopGuardMode::Note, 2),
            (TODAY, "qwen", LoopGuardMode::Note, 3),
        ]),
    )
    .await;

    let today_only = read(&pool, first_day_of_window(NOON, 1)).await;
    assert_eq!(today_only.len(), 1);
    assert_eq!((today_only[0].scanned, today_only[0].trips), (3, 0));

    let both = read(&pool, first_day_of_window(NOON, 2)).await;
    assert_eq!(both.len(), 2);
    assert_eq!(both[1].day, "2026-09-17", "newest day first");
    assert_eq!((both[1].scanned, both[1].trips), (2, 1));
}

#[tokio::test]
async fn days_older_than_the_retention_are_pruned_from_both_tables() {
    let pool = setup_test_database().await.unwrap();
    write(
        &pool,
        &[
            trip(
                NOON - 3 * DAY,
                "qwen",
                LoopGuardTrip::Loop,
                LoopGuardMode::Note,
            ),
            trip(NOON, "qwen", LoopGuardTrip::Loop, LoopGuardMode::Note),
        ],
        &scans(&[
            (TODAY - 3, "qwen", LoopGuardMode::Note, 1),
            (TODAY, "qwen", LoopGuardMode::Note, 1),
        ]),
    )
    .await;

    let mut conn = pool.acquire().await.unwrap();
    prune(&mut conn, NOON, 2, 50_000).await.unwrap();
    drop(conn);

    let days = read(&pool, TODAY - 10).await;
    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!(days[0].day, "2026-09-18");
    let (trips,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM loop_guard_trips")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(trips, 1);
}

#[tokio::test]
async fn the_row_cap_drops_whole_old_days_and_never_today() {
    let pool = setup_test_database().await.unwrap();
    let mut trips = Vec::new();
    for day_back in [2_u64, 1, 0] {
        for i in 0..2 {
            trips.push(trip(
                NOON - day_back * DAY + i,
                "qwen",
                LoopGuardTrip::Loop,
                LoopGuardMode::Note,
            ));
        }
    }
    write(
        &pool,
        &trips,
        &scans(&[
            (TODAY - 2, "qwen", LoopGuardMode::Note, 9),
            (TODAY - 1, "qwen", LoopGuardMode::Note, 9),
            (TODAY, "qwen", LoopGuardMode::Note, 9),
        ]),
    )
    .await;

    // Six trips, a cap of three: the fourth newest is yesterday's, so
    // yesterday and everything before it goes — trips and scans together, so
    // no day is left reading "scanned, never tripped".
    let mut conn = pool.acquire().await.unwrap();
    prune(&mut conn, NOON, 90, 3).await.unwrap();
    drop(conn);

    let days = read(&pool, TODAY - 10).await;
    let got: Vec<_> = days
        .iter()
        .map(|d| (d.day.as_str(), d.scanned, d.trips))
        .collect();
    assert_eq!(got, vec![("2026-09-18", 9, 2)]);

    // A cap today alone exceeds keeps today whole.
    let mut conn = pool.acquire().await.unwrap();
    prune(&mut conn, NOON, 90, 1).await.unwrap();
    drop(conn);
    assert_eq!(read(&pool, TODAY - 10).await[0].trips, 2);
}

#[tokio::test]
async fn no_text_a_trip_was_built_from_reaches_any_column() {
    let pool = setup_test_database().await.unwrap();
    write(
        &pool,
        &[
            trip(NOON, "model", LoopGuardTrip::Loop, LoopGuardMode::Note)
                .with_signature("SENTINELtool:00000000deadbeef|SENTINELtool:1")
                .with_session("sentinel-session"),
        ],
        &HashMap::new(),
    )
    .await;

    let (hits,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM loop_guard_trips WHERE \
         instr(lower(model_name), 'sentinel') OR instr(lower(gglib_version), 'sentinel') OR \
         instr(lower(mode), 'sentinel') OR instr(lower(detector), 'sentinel') OR \
         instr(lower(COALESCE(signature_hash, '')), 'sentinel') OR \
         instr(lower(COALESCE(session_hash, '')), 'sentinel')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(hits, 0);
    let (session,): (String,) = sqlx::query_as("SELECT session_hash FROM loop_guard_trips")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(session, "dbe38cf4ae365a2b");
}

#[tokio::test]
async fn a_trip_whose_scan_was_lost_still_shows() {
    let pool = setup_test_database().await.unwrap();
    let t = trip(NOON, "qwen", LoopGuardTrip::Loop, LoopGuardMode::Note);
    write(&pool, &[t], &HashMap::new()).await;

    let days = read(&pool, TODAY).await;
    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!((days[0].scanned, days[0].trips), (0, 1));
}
