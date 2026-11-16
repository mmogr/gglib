//! [`ProxyOps::start_as`]: a start that may find the endpoint on, and the
//! switch from one endpoint to another as one operation.
//!
//! A child of `proxy.rs`, so it reaches `ProxyOps`'s own fields.

use std::net::SocketAddr;

use gglib_core::ports::PinnedSpec;
use gglib_runtime::proxy::{ProxyConfig, ProxyStatus};

use super::ProxyOps;
use crate::error::GuiError;
use crate::proxy_switch::{ProxyShape, StartAnswer, StartOutcome, change, shape_words};

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
                match self.start_unlocked(config.clone(), pin.clone()).await {
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
        // Every start records what it started, and every stop clears it,
        // all under this lock: a running endpoint without a record is not
        // one this can compare, so it is left as it is.
        let Some(running) = running else {
            return Ok(StartAnswer::On(address, StartOutcome::Unchanged));
        };
        // Keys compare as the endpoint demands them: what it was started
        // with, against the key asked for or else the stored one.
        let stored = self.core.settings().get().await?.proxy_api_key;
        let running_key = self.effective_api_key().map(|(key, _)| key);
        let asked_key = config
            .api_key
            .clone()
            .or_else(|| gglib_core::secret::held(stored.as_deref()).map(str::to_owned));
        let now = ProxyShape::of(
            &running.config,
            running.pin.as_ref(),
            address.port(),
            running_key.as_deref(),
        );
        let asked = ProxyShape::of(&config, pin.as_ref(), config.port, asked_key.as_deref());
        let Some(differs) = change(&now, &asked) else {
            return Ok(StartAnswer::On(address, StartOutcome::Unchanged));
        };
        if !replace {
            return Ok(StartAnswer::Differs(differs));
        }
        let address = self.replace(running, config, pin).await?;
        Ok(StartAnswer::On(address, StartOutcome::Replaced))
    }

    /// Stop what runs and start `config` and `pin`; on a refusal, start
    /// `previous` again.
    async fn replace(
        &self,
        previous: Running,
        config: ProxyConfig,
        pin: Option<PinnedSpec>,
    ) -> Result<SocketAddr, GuiError> {
        // Every answer of a stop that found the endpoint on means it is off
        // now: the supervisor gives up its handle before it waits, and one
        // that outlasts its wait (a reply still streaming) is aborted. So
        // the switch goes on to its start.
        if let Err(e) = self.stop_unlocked().await {
            tracing::debug!("the switch's stop: {e}");
        }
        let refused = match self.start_unlocked(config, pin).await {
            Ok(address) => return Ok(address),
            Err(refused) => reason(refused),
        };
        if self
            .start_unlocked(previous.config, previous.pin)
            .await
            .is_err()
        {
            // Nothing this started runs, so no pin may outlive it.
            if let Err(e) = self.runtime.set_pin(None) {
                tracing::debug!("could not clear the model pin: {e}");
            }
        }
        // What is said is read from the endpoint as it is now, not from
        // which start answered.
        let now = match self.supervisor.status().await {
            ProxyStatus::Running { address } => {
                let pinned = self.runtime.pinned().map(|p| p.name);
                format!(
                    "The endpoint is back as it was, {}.",
                    shape_words(address.port(), pinned.as_deref())
                )
            }
            ProxyStatus::Stopped | ProxyStatus::Crashed => {
                "The endpoint is off now: turn it on again.".to_owned()
            }
        };
        Err(GuiError::Conflict(format!("{refused} {now}")))
    }
}

/// A refusal's own sentence, without the kind `GuiError` prefixes it with.
fn reason(refused: GuiError) -> String {
    match refused {
        GuiError::Conflict(message)
        | GuiError::Internal(message)
        | GuiError::ValidationFailed(message)
        | GuiError::Unavailable(message) => message,
        other => other.to_string(),
    }
}

#[cfg(test)]
#[path = "proxy_start_as_tests.rs"]
mod tests;
