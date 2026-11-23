//! The gate closes an allowance with none left until its reset, and opens
//! it then, at the moments the tests pass.

use std::time::Duration;

use tokio::time::Instant;

use super::*;

fn limit(bucket: HubBucket, remaining: u64, seconds: u64) -> HubLimit {
    HubLimit {
        bucket,
        remaining,
        resets_in: Duration::from_secs(seconds),
    }
}

#[test]
fn a_new_gate_is_open() {
    let gate = HubGate::new();
    let now = Instant::now();

    assert_eq!(gate.closed(HubBucket::Api, now), None);
    assert_eq!(gate.closed(HubBucket::Resolvers, now), None);
}

/// A refusal closes its allowance until its reset, and no other.
#[test]
fn a_refusal_closes_its_allowance_until_its_reset() {
    let gate = HubGate::new();
    let now = Instant::now();

    gate.note(limit(HubBucket::Api, 0, 10), now);

    assert_eq!(
        gate.closed(HubBucket::Api, now),
        Some(Duration::from_secs(10))
    );
    assert_eq!(
        gate.closed(HubBucket::Api, now + Duration::from_secs(4)),
        Some(Duration::from_secs(6))
    );
    assert_eq!(gate.closed(HubBucket::Resolvers, now), None);
    assert_eq!(gate.closed(HubBucket::Pages, now), None);
}

/// At its reset the allowance is open again.
#[test]
fn an_allowance_reopens_at_its_reset() {
    let gate = HubGate::new();
    let now = Instant::now();

    gate.note(limit(HubBucket::Api, 0, 10), now);

    assert_eq!(
        gate.closed(HubBucket::Api, now + Duration::from_secs(10)),
        None
    );
    assert_eq!(
        gate.closed(HubBucket::Api, now + Duration::from_secs(11)),
        None
    );
}

/// An answer that was served but left none closes the allowance too: the
/// next request would be refused.
#[test]
fn an_answer_that_leaves_none_closes_the_allowance() {
    let gate = HubGate::new();
    let now = Instant::now();

    gate.note(limit(HubBucket::Api, 0, 142), now);

    assert_eq!(
        gate.closed(HubBucket::Api, now),
        Some(Duration::from_secs(142))
    );
}

/// An answer with some left, read after a refusal, was made before it
/// (nothing is sent while an allowance is closed): it does not reopen an
/// allowance that still has time to wait.
#[test]
fn an_answer_with_some_left_does_not_reopen_the_allowance() {
    let gate = HubGate::new();
    let now = Instant::now();
    gate.note(limit(HubBucket::Api, 0, 10), now);

    gate.note(
        limit(HubBucket::Api, 485, 142),
        now + Duration::from_secs(1),
    );

    assert_eq!(
        gate.closed(HubBucket::Api, now + Duration::from_secs(1)),
        Some(Duration::from_secs(9))
    );
}

/// Of two closures, the later reset holds, whichever is read last.
#[test]
fn the_later_of_two_resets_holds() {
    let gate = HubGate::new();
    let now = Instant::now();

    gate.note(limit(HubBucket::Api, 0, 30), now);
    gate.note(limit(HubBucket::Api, 0, 10), now + Duration::from_secs(1));

    assert_eq!(
        gate.closed(HubBucket::Api, now),
        Some(Duration::from_secs(30))
    );

    gate.note(limit(HubBucket::Api, 0, 60), now + Duration::from_secs(2));

    assert_eq!(
        gate.closed(HubBucket::Api, now),
        Some(Duration::from_secs(62))
    );
}

/// A reset is held to fifteen minutes at most, and one second at least.
#[test]
fn a_reset_is_held_between_a_second_and_fifteen_minutes() {
    let gate = HubGate::new();
    let now = Instant::now();

    gate.note(limit(HubBucket::Api, 0, 86_400), now);
    gate.note(limit(HubBucket::Resolvers, 0, 0), now);

    assert_eq!(gate.closed(HubBucket::Api, now), Some(MAX_RESET));
    assert_eq!(MAX_RESET, Duration::from_mins(15));
    assert_eq!(gate.closed(HubBucket::Resolvers, now), Some(MIN_CLOSED));
}
