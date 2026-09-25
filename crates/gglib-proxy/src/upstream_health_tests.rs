//! Tests for [`super`]: what each verdict does to the strike streak, and the
//! one-shot recycle request.

use super::*;

#[test]
fn healthy_outcome_keeps_strikes_zero() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Healthy);
    h.record_stream_outcome(StreamVerdict::Healthy);
    assert_eq!(h.snapshot().consecutive_strikes, 0);
    assert!(!h.take_recycle_request());
}

#[test]
fn single_strike_does_not_trip_recycle() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Empty);
    assert_eq!(h.snapshot().consecutive_strikes, 1);
    assert!(!h.take_recycle_request());
}

#[test]
fn two_consecutive_strikes_trip_recycle_once() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Empty);
    h.record_timeout();
    assert!(h.take_recycle_request());
    // One-shot: a second consume returns false and the counter is reset.
    assert!(!h.take_recycle_request());
    assert_eq!(h.snapshot().consecutive_strikes, 0);
}

#[test]
fn a_healthy_outcome_resets_the_strike_streak() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Empty);
    h.record_stream_outcome(StreamVerdict::Healthy);
    h.record_stream_outcome(StreamVerdict::Empty);
    // Only one strike since the reset — threshold not reached.
    assert!(!h.take_recycle_request());
    assert_eq!(h.snapshot().consecutive_strikes, 1);
}

#[test]
fn cumulative_counters_track_events() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Empty); // empty #1, strike #1
    h.record_timeout(); // timeout #1, strike #2 → recycle armed
    assert!(h.take_recycle_request()); // recycle #1
    let snap = h.snapshot();
    assert_eq!(snap.total_empty_responses, 1);
    assert_eq!(snap.total_first_byte_timeouts, 1);
    assert_eq!(snap.total_recycles, 1);
    assert_eq!(snap.consecutive_strikes, 0);
}

/// The regression this module's four-state verdict exists for: a server
/// dying mid-stream used to arrive as "healthy", because the error frame
/// it emitted was renderable. Every request failing therefore held the
/// streak at zero and the recycle never fired.
#[test]
fn a_stream_that_dies_upstream_strikes_instead_of_resetting() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::UpstreamError);
    assert_eq!(h.snapshot().consecutive_strikes, 1);
    h.record_stream_outcome(StreamVerdict::UpstreamError);
    assert!(h.take_recycle_request());
    let snap = h.snapshot();
    assert_eq!(snap.total_upstream_errors, 2);
    // Not folded into the empty-response count — a crashing server and a
    // silent one are different illnesses.
    assert_eq!(snap.total_empty_responses, 0);
}

/// The other half: hanging up is a person's action. Two cancellations in a
/// row used to be indistinguishable from two empty responses, which at
/// `STRIKE_THRESHOLD == 2` was enough to recycle a healthy model server.
#[test]
fn a_client_hangup_neither_strikes_nor_resets() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Empty);
    h.record_stream_outcome(StreamVerdict::ClientAborted);
    h.record_stream_outcome(StreamVerdict::ClientAborted);
    // The one real strike still stands — abstaining is not forgiving.
    assert_eq!(h.snapshot().consecutive_strikes, 1);
    assert!(!h.take_recycle_request());
    assert_eq!(h.snapshot().total_client_aborts, 2);
}

/// A recycle that could not be carried out must not spend the watchdog's
/// case. Before this, a failed stop left the flag cleared and the streak
/// zeroed, so a server that was still sick got a clean slate and needed
/// two fresh strikes before anyone tried again.
#[test]
fn a_failed_recycle_rearms_instead_of_spending_the_request() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Empty);
    h.record_stream_outcome(StreamVerdict::Empty);
    assert!(h.take_recycle_request(), "threshold reached");

    // The stop failed, so the request goes back.
    h.rearm_recycle();
    assert!(
        h.take_recycle_request(),
        "the re-armed request is available to the next idle caller"
    );

    let snap = h.snapshot();
    assert_eq!(snap.total_recycle_failures, 1);
    // Both takes count as triggered — the failure is tracked separately
    // rather than by rewriting a cumulative counter.
    assert_eq!(snap.total_recycles, 2);
}

#[test]
fn rearming_is_not_needed_on_the_happy_path() {
    let h = UpstreamHealth::new();
    h.record_stream_outcome(StreamVerdict::Empty);
    h.record_stream_outcome(StreamVerdict::Empty);
    assert!(h.take_recycle_request());
    assert!(!h.take_recycle_request(), "still one-shot when it succeeds");
    assert_eq!(h.snapshot().total_recycle_failures, 0);
}

#[test]
fn client_aborts_alone_never_trip_a_recycle() {
    let h = UpstreamHealth::new();
    for _ in 0..STRIKE_THRESHOLD + 5 {
        h.record_stream_outcome(StreamVerdict::ClientAborted);
    }
    assert_eq!(h.snapshot().consecutive_strikes, 0);
    assert!(!h.take_recycle_request());
}
