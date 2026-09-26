//! What a device row reads as, for each state the roster, the key file and
//! the edge can leave it in, on a clock the test holds still.

use super::super::wire::RemoteDevice;
use super::{ago, describe};

/// The clock every row here is described at.
const NOW: i64 = 1_757_000_000_000;

/// A row with neither timestamp, which is the shape an unredeemed invite
/// leaves behind.
fn unredeemed() -> RemoteDevice {
    RemoteDevice {
        id: "dev-0a1b2c3d".to_owned(),
        label: None,
        joined_at: NOW - 3_600_000,
        redeemed_at: None,
        last_seen: None,
        peer: None,
        admitted: Some(true),
        recorded: true,
        description: String::new(),
        joined: false,
    }
}

/// #1034: a key the daemon holds with no roster row reads as one, and says
/// whether the edge admits it, because a key admitted with no record is the
/// row a person has to act on. With the tunnel down it says neither.
#[test]
fn a_key_with_no_record_says_so_and_whether_it_is_admitted() {
    let held = RemoteDevice {
        recorded: false,
        joined_at: 0,
        ..unredeemed()
    };
    assert_eq!(
        describe(&held, NOW),
        "key held, no record · admitted",
        "it was never an invite, so it says nothing of joining"
    );

    let refused = RemoteDevice {
        admitted: Some(false),
        ..held.clone()
    };
    assert_eq!(
        describe(&refused, NOW),
        "key held, no record · not admitted"
    );

    let down = RemoteDevice {
        admitted: None,
        ..held
    };
    assert_eq!(
        describe(&down, NOW),
        "key held, no record",
        "the tunnel being down singles out no row"
    );
}

/// An invite nobody took says so, rather than rendering as a device.
///
/// It is `admitted: true` — the key was minted and seeded at the edge before
/// the code was ever shown — so admission cannot be what distinguishes it.
#[test]
fn an_invite_nobody_took_says_it_was_never_joined() {
    assert_eq!(
        describe(&unredeemed(), NOW),
        "invited 1h ago, never joined",
        "an unspent invite reads as one, and says when it was offered, which \
         is all that is known of it"
    );
}

/// An invite the edge no longer admits says so, as well as saying nobody
/// took it; with the tunnel down it does not, because nothing is admitted
/// then and an invite is no more "not admitted" than any other row.
#[test]
fn an_invite_the_edge_no_longer_admits_says_both() {
    let refused = RemoteDevice {
        admitted: Some(false),
        ..unredeemed()
    };
    assert_eq!(
        describe(&refused, NOW),
        "invited 1h ago, never joined · not admitted",
        "still an invite nobody took, and the edge is refusing its key"
    );

    let down = RemoteDevice {
        admitted: None,
        ..unredeemed()
    };
    assert_eq!(
        describe(&down, NOW),
        "invited 1h ago, never joined",
        "the tunnel being down singles out no row"
    );
}

/// The second opinion. `redeemed_at` is written by a background task and can
/// be lost; `last_seen` is written by another. A device that has plainly made
/// requests must never be called one that never arrived, whichever write went
/// missing.
#[test]
fn a_device_that_has_been_seen_is_never_called_never_joined() {
    let seen = RemoteDevice {
        last_seen: Some(NOW - 120_000),
        ..unredeemed()
    };

    let line = describe(&seen, NOW);
    assert!(
        !line.contains("never joined"),
        "requests arrived under its key, so it joined: {line}"
    );
    assert!(line.contains("last seen 2m ago"), "{line}");
}

/// A redeemed device that has not yet made a request is the case the two
/// timestamps exist to tell apart from the one above.
#[test]
fn a_redeemed_device_with_no_requests_yet_is_not_never_joined() {
    let joined = RemoteDevice {
        label: Some("Matt's iPhone".to_owned()),
        redeemed_at: Some(NOW - 5_000),
        ..unredeemed()
    };

    let line = describe(&joined, NOW);
    assert!(!line.contains("never joined"), "{line}");
    assert!(
        line.contains("no requests yet"),
        "which is a different thing from never having joined: {line}"
    );
    assert!(
        !line.contains("Matt's iPhone"),
        "the name is printed beside the line, not in it: {line}"
    );
}

/// #1041: a redeemed device says which endpoint it paired from, when that was
/// recorded, and a row with no record of one says nothing about it.
#[test]
fn a_redeemed_device_says_which_endpoint_it_paired_from() {
    let joined = RemoteDevice {
        redeemed_at: Some(NOW - 5_000),
        peer: Some("3ca82708b995".to_owned()),
        ..unredeemed()
    };
    assert_eq!(
        describe(&joined, NOW),
        "no requests yet · paired from 3ca82708b995"
    );

    let unrecorded = RemoteDevice {
        peer: None,
        ..joined
    };
    assert_eq!(describe(&unrecorded, NOW), "no requests yet");
}

/// A device that joined, and whose key the edge refuses while the tunnel is
/// up, says it is not admitted.
#[test]
fn a_joined_device_the_edge_refuses_says_it_is_not_admitted() {
    let refused = RemoteDevice {
        redeemed_at: Some(NOW - 5_000),
        last_seen: Some(NOW - 5_000),
        admitted: Some(false),
        ..unredeemed()
    };
    assert_eq!(describe(&refused, NOW), "last seen just now · not admitted");
}

/// With the tunnel down nothing is admitted, so no single row is the one
/// that was dropped.
#[test]
fn a_row_is_not_called_unadmitted_merely_because_the_tunnel_is_down() {
    let down = RemoteDevice {
        redeemed_at: Some(NOW - 5_000),
        last_seen: Some(NOW - 5_000),
        admitted: None,
        ..unredeemed()
    };
    assert_eq!(
        describe(&down, NOW),
        "last seen just now · tunnel down",
        "not \"not admitted\", which would read as this device having been retired"
    );
}

/// A clock that moved backwards says so rather than claiming the future —
/// but a few seconds ahead is not that. The line is 61 whole seconds ahead.
#[test]
fn a_timestamp_ahead_of_the_clock_is_not_rendered_as_a_duration() {
    assert_eq!(ago(NOW + 600_000, NOW), "at an unknown time");
    assert_eq!(ago(NOW + 5_000, NOW), "just now", "five seconds is skew");
    assert_eq!(ago(NOW + 60_999, NOW), "just now");
    assert_eq!(ago(NOW + 61_000, NOW), "at an unknown time");
}

/// The suffix follows "last seen" as it follows "no requests yet": a device
/// that has made requests still says which endpoint it paired from.
#[test]
fn a_device_that_has_been_seen_still_says_which_endpoint_it_paired_from() {
    let seen = RemoteDevice {
        redeemed_at: Some(NOW - 600_000),
        last_seen: Some(NOW - 300_000),
        peer: Some("3ca82708b995".to_owned()),
        ..unredeemed()
    };
    assert_eq!(
        describe(&seen, NOW),
        "last seen 5m ago · paired from 3ca82708b995"
    );
}

/// A duration reads in the largest unit of which a whole one has passed, and
/// counts whole units of it: nothing is rounded up to a unit not yet reached.
#[test]
fn a_duration_reads_in_the_largest_whole_unit_that_has_passed() {
    for (before_ms, reads) in [
        (59_999, "just now"),
        (60_000, "1m ago"),
        (3_599_999, "59m ago"),
        (3_600_000, "1h ago"),
        (86_399_999, "23h ago"),
        (86_400_000, "1d ago"),
        (30 * 86_400_000, "30d ago"),
    ] {
        assert_eq!(ago(NOW - before_ms, NOW), reads, "{before_ms} ms before");
    }
}

/// An empty fingerprint names no endpoint, so a row carrying one says
/// nothing about where it paired from rather than "paired from " and a gap.
#[test]
fn an_empty_peer_is_no_peer() {
    let blank = RemoteDevice {
        redeemed_at: Some(NOW - 5_000),
        peer: Some(String::new()),
        ..unredeemed()
    };
    assert_eq!(describe(&blank, NOW), "no requests yet");
}
