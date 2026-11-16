//! [`ProxyOps::ensure_running_or_off`]: the endpoint brought up for a caller
//! that can go on without it, a model load (#2047).
//!
//! A child of `proxy.rs`, so it reaches `ProxyOps`'s own start.

use std::net::SocketAddr;

use gglib_runtime::proxy::SupervisorError;

use super::ProxyOps;
use crate::error::GuiError;

/// What a caller that wants the endpoint on, but does not need it, found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Endpoint {
    /// On, at this address.
    On(SocketAddr),
    /// Off, because another program holds its port: the status's own
    /// sentence for it ([`ProxyOps::refused`]).
    Off(String),
}

impl ProxyOps {
    /// [`Self::ensure_running`], but a port another program holds is an
    /// answer, not a failure: [`Endpoint::Off`] with the sentence the status
    /// carries, read under the same lock the start took, so a start racing
    /// in after it cannot have cleared it.
    ///
    /// Only that one goes on. Settings that cannot be read, and a start the
    /// supervisor fails for any other reason, are still errors: those leave
    /// nothing in the status to say why the endpoint is off.
    pub(crate) async fn ensure_running_or_off(&self) -> Result<Endpoint, GuiError> {
        let _one = self.switching.lock().await;
        match self.ensure_running_unlocked().await? {
            Ok(address) => Ok(Endpoint::On(address)),
            Err(e @ SupervisorError::BindFailed { .. }) => self
                .refused()
                .map(Endpoint::Off)
                .ok_or_else(|| Self::map_start_error(e)),
            Err(e) => Err(Self::map_start_error(e)),
        }
    }
}
