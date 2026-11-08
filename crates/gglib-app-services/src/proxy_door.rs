//! The door to this machine's endpoint: where a client on this machine
//! reaches it, and the key it demands.
//!
//! A chat run dials it today; a paired machine's door comes from its far
//! proxy (`remote::far_proxy`). Both are one [`Door`], so a refused key is
//! said in the door's own words wherever the turn came from.

use std::fmt;
use std::net::SocketAddr;

use gglib_core::access::reachable_at;
use gglib_runtime::Door;
use gglib_runtime::proxy::ProxyStatus;

use crate::error::GuiError;
use crate::proxy::ProxyOps;
use crate::remote::key::demanded;

/// What a request to the endpoint while it is off is told.
pub(crate) const ENDPOINT_OFF: &str =
    "the endpoint is off: turn it on first (`gglib proxy`, or Turn on)";

/// Why this machine's endpoint has no door to dial.
#[derive(Debug)]
pub enum DoorClosed {
    /// The endpoint is not running.
    Off,
    /// The settings that hold its key could not be read.
    Unreadable(GuiError),
}

impl fmt::Display for DoorClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str(ENDPOINT_OFF),
            Self::Unreadable(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for DoorClosed {}

impl From<DoorClosed> for GuiError {
    fn from(closed: DoorClosed) -> Self {
        match closed {
            DoorClosed::Off => Self::ValidationFailed(ENDPOINT_OFF.to_owned()),
            DoorClosed::Unreadable(e) => e,
        }
    }
}

impl ProxyOps {
    /// The running endpoint as a door: its root where a client on this
    /// machine reaches it (`gglib_core::access::reachable_at`, no `/v1`),
    /// and the key its bearer policy demands now
    /// (`remote::key::demanded`).
    ///
    /// The key is read fresh from settings and the endpoint reads it through
    /// a cache, so for up to `SETTINGS_CACHE_TTL` after the key is changed
    /// this door carries the new key while the endpoint still admits the old
    /// one. It is the key the endpoint converges to and then keeps; the
    /// door's own refusal sentence covers the window.
    ///
    /// # Errors
    ///
    /// [`DoorClosed::Off`] when the endpoint is not running (this never
    /// starts it), [`DoorClosed::Unreadable`] when settings cannot be read.
    pub async fn door(&self) -> Result<Door, DoorClosed> {
        let ProxyStatus::Running { address } = self.status().await else {
            return Err(DoorClosed::Off);
        };
        let settings = self.core.settings().get().await.map_err(|e| {
            DoorClosed::Unreadable(GuiError::from(e).context("could not read settings"))
        })?;
        let key = demanded(self.effective_api_key(), settings.proxy_api_key.as_deref());
        Ok(local_door(address, key))
    }
}

/// This machine's endpoint bound at `addr`, as a client on this machine
/// dials it, with `key`.
pub(crate) fn local_door(addr: SocketAddr, key: Option<String>) -> Door {
    Door {
        root_url: format!("http://{}", reachable_at(addr)),
        key,
        name: None,
    }
}

#[cfg(test)]
#[path = "proxy_door_tests.rs"]
mod proxy_door_tests;
