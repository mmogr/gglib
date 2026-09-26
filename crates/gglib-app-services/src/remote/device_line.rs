//! How a device row is described to a person: the words in
//! [`RemoteDevice::description`] and the answer in [`RemoteDevice::joined`].
//!
//! Written here, on the daemon's clock, so the CLI and the GUI print one
//! description of one roster rather than each deriving its own.

use super::wire::RemoteDevice;

/// Whether a device has arrived under `d`'s key.
///
/// **A row is only called never-joined when both timestamps are empty.**
/// `redeemed_at` and `last_seen` are written by separate background tasks and
/// either write can be lost, so a device that has plainly made requests must
/// not read as one that never arrived.
pub(super) fn joined(d: &RemoteDevice) -> bool {
    d.redeemed_at.is_some() || d.last_seen.is_some()
}

/// What is known of `d` as of `now_ms`, its facts joined by ` · `.
///
/// The name is not part of it: a surface prints the label and the id beside
/// this, and they are the same whatever state the row is in.
pub(super) fn describe(d: &RemoteDevice, now_ms: i64) -> String {
    // A key this machine holds that no roster row lists (#1034): its id and
    // whether the edge admits it are all that is known. Said first, because
    // every description below reads a roster row.
    if !d.recorded {
        return match d.admitted {
            Some(true) => "key held, no record · admitted".to_owned(),
            Some(false) => "key held, no record · not admitted".to_owned(),
            None => "key held, no record".to_owned(),
        };
    }
    if !joined(d) {
        // An invite the edge has stopped honouring is still one nobody took,
        // and worth saying twice: an unwind that failed part-way leaves
        // exactly this row, and "never joined" alone reads as a harmless
        // unspent code. With the tunnel down no row is admitted, so there is
        // nothing to add.
        let invited = format!("invited {}, never joined", ago(d.joined_at, now_ms));
        return if d.admitted == Some(false) {
            format!("{invited} · not admitted")
        } else {
            invited
        };
    }
    let mut line = match d.last_seen {
        Some(at) => format!("last seen {}", ago(at, now_ms)),
        None => "no requests yet".to_owned(),
    };
    // Where the invite was redeemed from, when that was recorded (#1041). An
    // empty fingerprint names no endpoint, so it is no peer.
    if let Some(peer) = d.peer.as_deref().filter(|peer| !peer.is_empty()) {
        line.push_str(" · paired from ");
        line.push_str(peer);
    }
    match d.admitted {
        Some(true) => {}
        Some(false) => line.push_str(" · not admitted"),
        // The tunnel is down, so nothing is admitted and this row is not
        // singled out for it.
        None => line.push_str(" · tunnel down"),
    }
    line
}

/// How long before `now_ms` the stamp `at_ms` is, in the largest whole unit
/// that has passed, for a line a person scans rather than measures.
///
/// A stamp 61 seconds or more ahead of the clock reads "at an unknown time":
/// "in the future" is a wrong answer a person can act on. One less than 61
/// seconds ahead reads "just now".
pub(super) fn ago(at_ms: i64, now_ms: i64) -> String {
    let secs = now_ms.saturating_sub(at_ms) / 1000;
    if secs < -60 {
        return "at an unknown time".to_owned();
    }
    match secs {
        s if s < 60 => "just now".to_owned(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
#[path = "device_line_tests.rs"]
mod device_line_tests;
