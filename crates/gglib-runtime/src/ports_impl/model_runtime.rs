//! ModelRuntimePort implementation using ProcessManager.
//!
//! This adapter wraps the ProcessManager (SingleSwap strategy) to implement
//! the ModelRuntimePort interface from gglib-core.

use async_trait::async_trait;
use gglib_core::cache_config::CacheRamSetting;
use gglib_core::ports::{ModelRuntimeError, ModelRuntimePort, RunningTarget};
use std::fmt;
use std::sync::Arc;

use crate::process::ProcessManager;

/// Implementation of ModelRuntimePort using ProcessManager.
///
/// Wraps a ProcessManager with SingleSwap strategy to provide
/// the runtime port interface for the proxy.
///
/// # Note
///
/// The ProcessManager is wrapped in Arc because it uses internal
/// synchronization (RwLock on current state, AtomicBool for loading).
/// This avoids "copied state" bugs and keeps everything honest.
pub struct RuntimePortImpl {
    /// The underlying process manager (must be SingleSwap strategy).
    mgr: Arc<ProcessManager>,
    /// Per-instance override for the host-RAM prompt cache setting, passed
    /// to every `ensure_model_running` call via
    /// `ProcessManager::ensure_model_running_with`. `None` defers to
    /// whatever the shared `mgr` was constructed with.
    cache_ram_override: Option<CacheRamSetting>,
}

impl RuntimePortImpl {
    /// Create a new RuntimePortImpl.
    ///
    /// # Arguments
    ///
    /// * `mgr` - ProcessManager configured with SingleSwap strategy
    pub fn new(mgr: Arc<ProcessManager>) -> Self {
        Self {
            mgr,
            cache_ram_override: None,
        }
    }

    /// Create a `RuntimePortImpl` that overrides the host-RAM prompt cache
    /// setting on every launch, independent of what `mgr` was constructed
    /// with.
    ///
    /// Lets one shared `ProcessManager` (single-model-at-a-time, so only one
    /// llama-server ever runs) serve callers with different cache-RAM needs
    /// — e.g. a GUI's proxy (`CacheRamSetting::Auto`, parity with the CLI
    /// proxy) and its benchmark runner (`CacheRamSetting::ExplicitMb(0)`,
    /// which must never gain a prompt cache) — without splitting the
    /// manager and losing the single-process guarantee.
    pub fn with_cache_ram(mgr: Arc<ProcessManager>, setting: CacheRamSetting) -> Self {
        Self {
            mgr,
            cache_ram_override: Some(setting),
        }
    }
}

impl fmt::Debug for RuntimePortImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimePortImpl").finish()
    }
}

#[async_trait]
impl ModelRuntimePort for RuntimePortImpl {
    async fn ensure_model_running(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        self.mgr
            .ensure_model_running_with(model_name, num_ctx, default_ctx, self.cache_ram_override)
            .await
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        self.mgr.current_model().await
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.mgr.stop_current().await
    }
}

#[cfg(test)]
mod tests {
    // Note: Full tests require a mock ProcessManager, which is complex to set up.
    // The real integration testing happens in the contract tests for gglib-proxy.
}
