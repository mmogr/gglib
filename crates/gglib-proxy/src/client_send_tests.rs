//! The production bounds equal the twins they are sized from, and a client
//! let go stays let go.

use std::time::Duration;

use super::*;
use crate::upstream_read::StreamBounds;

#[test]
fn the_production_bounds_equal_the_twins_they_are_sized_from() {
    let bounds = StreamBounds::default();

    // The client is given as long to take a frame as the upstream to send one.
    assert_eq!(bounds.send, STREAM_IDLE_TIMEOUT);
}

#[tokio::test(start_paused = true)]
async fn once_a_send_has_waited_out_the_bound_every_later_send_fails_at_once() {
    let bound = Duration::from_secs(1);
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let client = ClientSender::new(tx, bound);
    assert!(client.send(Bytes::from_static(b"a")).await, "there is room");

    let started = tokio::time::Instant::now();
    assert!(!client.send(Bytes::from_static(b"b")).await);
    assert_eq!(started.elapsed(), bound, "the full send waited the bound");

    let started = tokio::time::Instant::now();
    assert!(!client.send(Bytes::from_static(b"c")).await);
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "the next one did not wait"
    );

    // Still open, so the sends failed on the bound and not on a departure.
    assert!(rx.try_recv().is_ok());
}
