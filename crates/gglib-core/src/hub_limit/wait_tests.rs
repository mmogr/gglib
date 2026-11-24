//! The sentence a wait is shown in, and the wait itself on tokio's paused
//! clock.

use std::time::Duration;

use tokio::time::Instant;

use super::*;

#[test]
fn the_sentence_names_the_time_left_in_minutes_and_seconds() {
    assert_eq!(
        waiting_sentence(Duration::from_secs(134)),
        "waiting for Hugging Face's rate limit, 2:14"
    );
    assert_eq!(
        waiting_sentence(Duration::from_secs(1)),
        "waiting for Hugging Face's rate limit, 0:01"
    );
    assert_eq!(
        waiting_sentence(Duration::from_mins(10)),
        "waiting for Hugging Face's rate limit, 10:00"
    );
    assert_eq!(
        waiting_sentence(MAX_RESET),
        "waiting for Hugging Face's rate limit, 15:00"
    );
}

/// Part of a second left rounds up: a wait never reads 0:00.
#[test]
fn a_part_of_a_second_rounds_up() {
    assert_eq!(clock(Duration::from_millis(1)), "0:01");
    assert_eq!(clock(Duration::from_millis(59_001)), "1:00");
    assert_eq!(clock(Duration::from_millis(133_500)), "2:14");
    assert_eq!(whole_seconds(Duration::from_millis(9_001)), 10);
    assert_eq!(whole_seconds(Duration::from_secs(10)), 10);
}

/// The wait is told the time left at its start and once a second, and ends
/// at its end, not cancelled.
#[tokio::test(start_paused = true)]
async fn a_wait_ticks_each_second_until_its_end() {
    let began = Instant::now();
    let mut ticks = Vec::new();

    let cancelled = wait_out(
        Duration::from_millis(2500),
        std::future::pending(),
        |left| ticks.push(left),
    )
    .await;

    assert!(!cancelled);
    assert_eq!(
        ticks,
        [
            Duration::from_millis(2500),
            Duration::from_millis(1500),
            Duration::from_millis(500)
        ]
    );
    let shown: Vec<String> = ticks.into_iter().map(clock).collect();
    assert_eq!(shown, ["0:03", "0:02", "0:01"]);
    assert_eq!(began.elapsed(), Duration::from_millis(2500));
}

/// A cancel ends the wait where it is, and says so.
#[tokio::test(start_paused = true)]
async fn a_cancel_ends_the_wait() {
    let began = Instant::now();
    let mut ticks = Vec::new();

    let cancelled = wait_out(
        Duration::from_secs(10),
        tokio::time::sleep(Duration::from_millis(1500)),
        |left| ticks.push(left),
    )
    .await;

    assert!(cancelled);
    assert_eq!(ticks, [Duration::from_secs(10), Duration::from_secs(9)]);
    assert_eq!(began.elapsed(), Duration::from_millis(1500));
}

/// A cancel that has already come ends the wait before any sleep.
#[tokio::test(start_paused = true)]
async fn a_wait_already_cancelled_does_not_sleep() {
    let began = Instant::now();

    let cancelled = wait_out(Duration::from_secs(10), async {}, |_| {}).await;

    assert!(cancelled);
    assert_eq!(began.elapsed(), Duration::ZERO);
}

/// Nothing to wait is no tick and no sleep.
#[tokio::test(start_paused = true)]
async fn nothing_to_wait_is_no_tick() {
    let mut ticks = Vec::new();

    let cancelled = wait_out(Duration::ZERO, std::future::pending(), |left| {
        ticks.push(left);
    })
    .await;

    assert!(!cancelled);
    assert!(ticks.is_empty());
}
