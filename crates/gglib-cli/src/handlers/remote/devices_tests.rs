//! What a roster row reads as in the terminal.

use super::*;

/// A row with neither timestamp, which is the shape an unredeemed invite
/// leaves behind.
fn unredeemed() -> RemoteDeviceDto {
    RemoteDeviceDto {
        id: "dev-0a1b2c3d".to_owned(),
        label: None,
        joined_at: now() - 3_600_000,
        redeemed_at: None,
        last_seen: None,
        admitted: Some(true),
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// An invite nobody took says so, rather than rendering as a device.
///
/// It is `admitted: true` — the key was minted and seeded at the edge before
/// the code was ever shown — so admission cannot be what distinguishes it.
#[test]
fn an_invite_nobody_took_says_it_was_never_joined() {
    let line = describe(&unredeemed());
    assert!(
        line.contains("never joined"),
        "an unspent invite has to read as one: {line}"
    );
    assert!(
        line.contains("invited 1h ago"),
        "and say when it was offered, which is all that is known of it: {line}"
    );
}

/// The second opinion. `redeemed_at` is written by a background task and can
/// be lost; `last_seen` is written by another. A device that has plainly made
/// requests must never be called one that never arrived, whichever write went
/// missing.
#[test]
fn a_device_that_has_been_seen_is_never_called_never_joined() {
    let seen = RemoteDeviceDto {
        last_seen: Some(now() - 120_000),
        ..unredeemed()
    };

    let line = describe(&seen);
    assert!(
        !line.contains("never joined"),
        "requests arrived under its key, so it joined: {line}"
    );
    assert!(line.contains("last seen 2m ago"), "{line}");
}

/// A redeemed device that has not yet made a request is the case the two
/// flags exist to tell apart from the one above.
#[test]
fn a_redeemed_device_with_no_requests_yet_is_not_never_joined() {
    let joined = RemoteDeviceDto {
        label: Some("Matt's iPhone".to_owned()),
        redeemed_at: Some(now() - 5_000),
        ..unredeemed()
    };

    let line = describe(&joined);
    assert!(!line.contains("never joined"), "{line}");
    assert!(line.contains("Matt's iPhone"), "{line}");
    assert!(
        line.contains("no requests yet"),
        "which is a different thing from never having joined: {line}"
    );
}

/// With the tunnel down nothing is admitted, so no single row is the one
/// that was dropped.
#[test]
fn a_row_is_not_called_unadmitted_merely_because_the_tunnel_is_down() {
    let down = RemoteDeviceDto {
        redeemed_at: Some(now() - 5_000),
        last_seen: Some(now() - 5_000),
        admitted: None,
        ..unredeemed()
    };

    let line = describe(&down);
    assert!(line.contains("tunnel down"), "{line}");
    assert!(
        !line.contains("not admitted"),
        "that would read as this device having been retired: {line}"
    );
}

/// A clock that moved backwards says so rather than claiming the future.
#[test]
fn a_timestamp_ahead_of_the_clock_is_not_rendered_as_a_duration() {
    assert_eq!(ago(now() + 600_000), "at an unknown time");
}
