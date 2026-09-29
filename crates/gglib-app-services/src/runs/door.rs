//! Where a chat run posts, and the key it presents.
//!
//! The daemon's own proxy, reached on loopback. The key follows the tunnel's
//! rule, shared from `remote::key`: what the running proxy demands, else the
//! stored `proxy_api_key`, which a proxy started with no key picks up once
//! one is stored. A run never mints a key.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::runs::RunError;
use gglib_core::services::AppCore;
use gglib_runtime::proxy::ProxyStatus;

use crate::proxy::ProxyOps;
use crate::remote::key::enforced;

/// A running proxy: where it listens and the key it wants.
pub(crate) struct Door {
    pub(crate) addr: SocketAddr,
    pub(crate) key: Option<String>,
}

/// Finds the proxy a chat run posts to.
#[async_trait]
pub(crate) trait ProxyDoor: Send + Sync {
    /// The running proxy, or `None` when it is not running.
    async fn open(&self) -> Result<Option<Door>, RunError>;
}

/// The daemon's own proxy.
pub(crate) struct LocalProxy {
    pub(crate) proxy: Arc<ProxyOps>,
    pub(crate) core: Arc<AppCore>,
}

#[async_trait]
impl ProxyDoor for LocalProxy {
    async fn open(&self) -> Result<Option<Door>, RunError> {
        let ProxyStatus::Running { address } = self.proxy.status().await else {
            return Ok(None);
        };
        Ok(Some(Door {
            addr: address,
            key: bearer(&self.proxy, &self.core).await?,
        }))
    }
}

/// The key a request to the local proxy must carry, or `None` when it
/// demands none.
pub(crate) async fn bearer(proxy: &ProxyOps, core: &AppCore) -> Result<Option<String>, RunError> {
    let settings = core.settings().get().await.map_err(|_| RunError {
        code: "internal_error".to_owned(),
        message: "The daemon could not read its settings.".to_owned(),
    })?;
    Ok(enforced(proxy.effective_api_key(), settings.proxy_api_key.as_deref()).map(|(key, _)| key))
}

/// The address to dial: a proxy bound to every interface is dialled on
/// 127.0.0.1.
pub(crate) fn dial(addr: SocketAddr) -> SocketAddr {
    if addr.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
    } else {
        addr
    }
}
