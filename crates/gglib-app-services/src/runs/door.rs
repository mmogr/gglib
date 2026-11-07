//! Where a chat run posts, and the key it presents.
//!
//! The daemon's own endpoint, as [`ProxyOps::door`] opens it: reached where
//! `gglib_core::access::reachable_at` says (a wildcard bind on loopback),
//! with the key its bearer policy demands now (`remote::key::demanded`). A
//! run never mints a key and never starts the endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::runs::RunError;
use gglib_runtime::Door;

use crate::DoorClosed;
use crate::proxy::ProxyOps;

/// Finds the proxy a chat run posts to.
#[async_trait]
pub(crate) trait ProxyDoor: Send + Sync {
    /// The running proxy, or `None` when it is not running.
    async fn open(&self) -> Result<Option<Door>, RunError>;
}

/// The daemon's own proxy.
pub(crate) struct LocalProxy {
    pub(crate) proxy: Arc<ProxyOps>,
}

#[async_trait]
impl ProxyDoor for LocalProxy {
    async fn open(&self) -> Result<Option<Door>, RunError> {
        match self.proxy.door().await {
            Ok(door) => Ok(Some(door)),
            Err(DoorClosed::Off) => Ok(None),
            Err(DoorClosed::Unreadable(_)) => Err(RunError {
                code: "internal_error".to_owned(),
                message: "The daemon could not read its settings.".to_owned(),
            }),
        }
    }
}
