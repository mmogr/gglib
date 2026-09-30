//! The daemon's ports a paired device reaches through the proxy.

use std::sync::Arc;

use crate::ports::{HubChatsPort, RunsPort};

/// What the daemon hands every proxy it starts for its paired devices.
///
/// Not an access rule: these ride with the access policy because that is
/// the one value the supervisor hands the proxy its live ports in. Each
/// absent one answers its routes 503, as an embedded server or a test has
/// none.
#[derive(Debug, Clone, Default)]
pub struct DevicePorts {
    /// The daemon's runs, at `/v1/runs`.
    pub runs: Option<Arc<dyn RunsPort>>,
    /// The hub's chats, at `/v1/chats`.
    pub chats: Option<Arc<dyn HubChatsPort>>,
}
