//! How the tunnel's state is said to a person: [`remote_status_lines`], the
//! words in [`RemoteStatus::sharing`], [`RemoteStatus::sharing_note`],
//! [`RemoteStatus::reach_line`] and [`RemoteStatus::reach_note`].
//!
//! Written once here so `gglib remote status`, the Devices place, the tray and
//! the sidebar say one thing. Each surface still adds what only it has: the
//! CLI its commands, the GUI its buttons and the time a machine went away.

use gglib_core::domain::ReachState;

use super::wire::RemoteStatus;

/// What is said of the tunnel, before [`RemoteStatus::with_lines`] puts it on
/// the status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteStatusLines {
    pub(super) sharing: String,
    pub(super) sharing_note: String,
    pub(super) reach: String,
    pub(super) reach_note: String,
}

/// What is said of `status`'s sharing and of the machine it has joined.
pub(super) fn remote_status_lines(status: &RemoteStatus) -> RemoteStatusLines {
    RemoteStatusLines {
        sharing: sharing(status),
        sharing_note: sharing_note(status),
        reach: reach(status),
        reach_note: reach_note(status),
    }
}

/// Whether this machine is shared, in a few words.
///
/// "Paired" counts the devices that have arrived under their key: an invite
/// nobody took is a row, but no device is paired by it.
fn sharing(status: &RemoteStatus) -> String {
    if status.enabled {
        let paired = status.devices.iter().filter(|d| d.joined).count();
        format!("Sharing \u{b7} {paired} paired")
    } else if status.remote_enabled {
        "Switched on, nothing bound".to_owned()
    } else {
        "Not sharing".to_owned()
    }
}

/// The sentence for a switch on with nothing bound, else nothing.
///
/// Worth its own sentence: it is a machine still arming, or one that failed
/// to, and "not sharing" alone would send someone to turn on a thing that is
/// already on.
fn sharing_note(status: &RemoteStatus) -> String {
    if !status.enabled && status.remote_enabled {
        "Switched on, but nothing is bound \u{2014} still arming, or it could not reach a relay."
            .to_owned()
    } else {
        String::new()
    }
}

/// The machine this one has joined, and whether it can be reached in the
/// state's own word; empty while none is joined.
///
/// Away, it ends at the word, so how long follows it directly: each surface
/// says that itself, the CLI as it prints and the GUI as a time that does not
/// go stale between reads.
fn reach(status: &RemoteStatus) -> String {
    let Some(connection) = &status.connected else {
        return String::new();
    };
    let machine = status.paired_shown();
    if connection.away_for_s.is_some() {
        format!("{machine} \u{b7} {}", ReachState::Away)
    } else {
        format!(
            "{machine} \u{b7} {} \u{b7} {}",
            ReachState::Connected,
            connection.path
        )
    }
}

/// What becomes of a machine that is away, said after how long; empty
/// otherwise.
fn reach_note(status: &RemoteStatus) -> String {
    if status
        .connected
        .as_ref()
        .is_some_and(|c| c.away_for_s.is_some())
    {
        "The address stays, and it reconnects when that machine is back.".to_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
#[path = "status_lines_tests.rs"]
mod status_lines_tests;
