//! Which of the servers open to paired devices a device's run may use.

use std::collections::BTreeSet;

/// The MCP servers a paired device's run may use, of those open to paired
/// devices: every one, or only those whose ids it names.
///
/// It only ever narrows. A server it admits is started only when it is also
/// open to devices, so an id it names that is closed, or that no server has,
/// starts nothing. Its default admits no server, so a run that forgets to
/// say is offered none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceServers {
    /// Every server open to paired devices: a turn on a chat this machine
    /// keeps, whose own settings narrow the tools.
    All,
    /// Only the open servers with these ids: a chat a phone keeps, which
    /// names them. Empty, none.
    Only(BTreeSet<i64>),
}

impl Default for DeviceServers {
    fn default() -> Self {
        Self::none()
    }
}

impl DeviceServers {
    /// No server at all.
    #[must_use]
    pub const fn none() -> Self {
        Self::Only(BTreeSet::new())
    }

    /// Whether this admits no server.
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::Only(ids) if ids.is_empty())
    }

    /// Whether this admits the server `id`.
    #[must_use]
    pub fn admits(&self, id: i64) -> bool {
        match self {
            Self::All => true,
            Self::Only(ids) => ids.contains(&id),
        }
    }
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
