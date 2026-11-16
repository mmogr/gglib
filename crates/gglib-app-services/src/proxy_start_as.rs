//! [`ProxyOps::start_as`]: a start that may find the endpoint on, and the
//! switch from one endpoint to another as one operation.
//!
//! A child of `proxy.rs`, so it reaches `ProxyOps`'s own fields.

use gglib_core::ports::PinnedSpec;
use gglib_runtime::proxy::{ProxyConfig, ProxyStatus};

use super::ProxyOps;
use crate::error::GuiError;
use crate::proxy_switch::{ProxyShape, StartAnswer, StartOutcome, change};

/// What a running endpoint was started with, kept by the start that did it.
#[derive(Clone)]
pub(super) struct Running {
    pub(super) config: ProxyConfig,
    pub(super) pin: Option<PinnedSpec>,
}

impl ProxyOps {
    /// Start the endpoint as `config` and `pin` ask, whatever it is doing.
    ///
    /// Off, it is started. On as asked, nothing changes. On otherwise, it is
    /// left alone and the answer says how it differs, unless `replace` is
    /// set: then it is stopped and started as asked in one operation, so
    /// `proxy_stopped` and `proxy_started` are each said once, by
    /// [`Self::stop`] and the start. Should the new start fail, the endpoint
    /// is put back as it was, and the refusal says which state it is in.
    ///
    /// # Errors
    ///
    /// Whatever [`Self::start`] refuses with when the endpoint is off; for a
    /// replacement whose start fails, a `Conflict` naming the reason and
    /// whether the endpoint is back as it was or off.
    pub async fn start_as(
        &self,
        config: ProxyConfig,
        pin: Option<PinnedSpec>,
        replace: bool,
    ) -> Result<StartAnswer, GuiError> {
        let _one = self.switching.lock().await;
        let address = match self.supervisor.status().await {
            ProxyStatus::Running { address } => address,
            ProxyStatus::Stopped | ProxyStatus::Crashed => {
                match self.start(config.clone(), pin.clone()).await {
                    Ok(address) => return Ok(StartAnswer::On(address, StartOutcome::Started)),
                    // Another start, not under this lock, got there first:
                    // compare with what it started.
                    Err(GuiError::Conflict(refused)) => match self.supervisor.status().await {
                        ProxyStatus::Running { address } => address,
                        _ => return Err(GuiError::Conflict(refused)),
                    },
                    Err(other) => return Err(other),
                }
            }
        };
        let running = self
            .running
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(running) = running else {
            return Ok(StartAnswer::On(address, StartOutcome::Unchanged));
        };
        let now = ProxyShape::of(&running.config, running.pin.as_ref(), address.port());
        let asked = ProxyShape::of(&config, pin.as_ref(), config.port);
        // Today: a start that finds the endpoint on is success, whatever it
        // asked, and nothing is replaced.
        let _ = (change(&now, &asked), replace);
        Ok(StartAnswer::On(address, StartOutcome::Unchanged))
    }
}

#[cfg(test)]
#[path = "proxy_start_as_tests.rs"]
mod tests;
