//! The daemon's ports a paired device reaches through the proxy.

use std::sync::Arc;

use crate::ports::{AgentRunStarter, HubChatsPort, ImageGenerationPort, RunsPort};

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
    /// What starts a device's turn on a hub chat, at
    /// `PUT /v1/runs/{id}?kind=agent`.
    pub turns: Option<Arc<dyn AgentRunStarter>>,
    /// The daemon's image driver, at `POST /v1/images/generations` for every
    /// client the proxy serves, local or paired.
    pub images: Option<Arc<dyn ImageGenerationPort>>,
}
