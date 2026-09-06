//! Constructors for the remote-tunnel events.
//!
//! Split from `mod.rs` for the file-size gate, the way the proxy
//! constructors would have been had they arrived later. Nothing here is a
//! secret: the ticket travels as its fingerprint and never whole, because
//! the event stream is readable by any local GUI client.

use super::AppEvent;

impl AppEvent {
    /// Create a [`AppEvent::RemoteEnabled`] event.
    pub const fn remote_enabled(ticket_fingerprint: String) -> Self {
        Self::RemoteEnabled { ticket_fingerprint }
    }

    /// Create a [`AppEvent::RemoteDisabled`] event.
    pub const fn remote_disabled() -> Self {
        Self::RemoteDisabled
    }

    /// Create a [`AppEvent::RemotePaired`] event.
    pub const fn remote_paired(peer: Option<String>) -> Self {
        Self::RemotePaired { peer }
    }

    /// Create a [`AppEvent::RemoteConnected`] event.
    pub const fn remote_connected(port: u16) -> Self {
        Self::RemoteConnected { port }
    }

    /// Create a [`AppEvent::RemoteDisconnected`] event.
    pub const fn remote_disconnected() -> Self {
        Self::RemoteDisconnected
    }
}
