//! Device rows as the surfaces see them: the roster, the key file and the
//! edge, joined.
//!
//! Its own file rather than more of `devices.rs`, which is at its size budget,
//! and along a seam: `devices.rs` is the three verbs, and this is the one
//! answer `list` and `status` both give, from three stores that can disagree.

use gglib_core::Device;

use super::device_line::{describe, joined};
use super::wire::RemoteDevice;

/// Roster rows as the surfaces see them, given what the edge is admitting,
/// then a row for each key `held` that no roster row lists, each described
/// as of `now_ms`.
///
/// Shared by [`RemoteOps::list`](super::RemoteOps::list) and
/// [`RemoteOps::status`](super::RemoteOps::status). Two spellings of "is this
/// device admitted" would be two chances to answer it differently, on the one
/// question a person is asking the list.
///
/// Both callers read settings and the key file before the serve slot, so
/// `admitted` is what the edge held when the slot was read — the later of the
/// reads. Mid-`forget` a row can come back "not admitted" for one read before
/// it is gone; a device the edge had already dropped by then is never called
/// admitted.
///
/// **A key with no row is listed, as `recorded: false`** (#1034). Nothing
/// else is known about it, and it is the row a person most needs to see: the
/// edge may be admitting it, and no other surface names it.
pub(super) fn viewed(
    roster: Vec<Device>,
    held: &[String],
    admitting: Option<&[String]>,
    now_ms: i64,
) -> Vec<RemoteDevice> {
    // `None` with the tunnel down: nothing admits then, and saying `false`
    // would read as "this device was dropped".
    let admitted = |id: &str| admitting.map(|names| names.iter().any(|name| name == id));
    let unrecorded: Vec<RemoteDevice> = held
        .iter()
        .filter(|id| !roster.iter().any(|row| &row.id == *id))
        .map(|id| RemoteDevice {
            admitted: admitted(id),
            id: id.clone(),
            label: None,
            joined_at: 0,
            redeemed_at: None,
            last_seen: None,
            peer: None,
            recorded: false,
            description: String::new(),
            joined: false,
        })
        .collect();
    roster
        .into_iter()
        .map(|d| RemoteDevice {
            admitted: admitted(&d.id),
            id: d.id,
            label: d.label,
            joined_at: d.joined_at,
            redeemed_at: d.redeemed_at,
            last_seen: d.last_seen,
            peer: d.peer,
            recorded: true,
            description: String::new(),
            joined: false,
        })
        .chain(unrecorded)
        .map(|mut row| {
            row.joined = joined(&row);
            row.description = describe(&row, now_ms);
            row
        })
        .collect()
}

#[cfg(test)]
#[path = "device_view_tests.rs"]
mod device_view_tests;
