//! ModelRuntimePort implementation using ProcessManager.
//!
//! This adapter wraps the ProcessManager (SingleSwap strategy) to implement
//! the ModelRuntimePort interface from gglib-core.

use async_trait::async_trait;
use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::AdmissionSnapshot;
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, ProcessHandle, RunningTarget,
};
use std::fmt;
use std::sync::Arc;

use crate::process::ProcessManager;

/// Implementation of ModelRuntimePort using ProcessManager.
///
/// # Note
///
/// The ProcessManager is wrapped in Arc because it uses internal
/// synchronization (a mutex over the admission queue, an RwLock over the
/// process table). This avoids "copied state" bugs and keeps everything honest.
pub struct RuntimePortImpl {
    /// The underlying process manager.
    mgr: Arc<ProcessManager>,
    /// Per-instance override for the host-RAM prompt cache setting, passed to
    /// every `admit` call. `None` defers to whatever the shared `mgr` was
    /// constructed with.
    cache_ram_override: Option<CacheRamSetting>,
}

impl RuntimePortImpl {
    /// Create a new RuntimePortImpl.
    ///
    /// # Arguments
    ///
    /// * `mgr` - the shared process manager
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
    /// Lets one shared `ProcessManager` — and therefore one admission queue
    /// governing every llama-server on the machine — serve callers with
    /// different cache-RAM needs: a GUI's proxy (`CacheRamSetting::Auto`,
    /// parity with the CLI proxy) and its benchmark runner
    /// (`CacheRamSetting::ExplicitMb(0)`, which must never gain a prompt cache)
    /// without splitting the manager and losing that single point of control.
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
    async fn admit(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
        mut overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        // This instance's standing cache-RAM setting applies only when the
        // caller expressed no preference of its own, so a per-call override
        // still wins over `with_cache_ram`.
        overrides.cache_ram = overrides.cache_ram.or(self.cache_ram_override);

        self.mgr
            .admit(model_name, num_ctx, default_ctx, overrides)
            .await
    }

    fn admission_snapshot(&self) -> AdmissionSnapshot {
        self.mgr.admission_snapshot()
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        self.mgr.current_model()
    }

    async fn list_running(&self) -> Vec<ProcessHandle> {
        self.mgr.list_running().await
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.mgr.stop_current().await
    }

    fn pinned_model(&self) -> Option<String> {
        self.mgr.pinned_model()
    }

    fn set_pin(&self, pin: Option<gglib_core::ports::PinnedSpec>) -> Result<(), ModelRuntimeError> {
        self.mgr.set_pin(pin);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Note: Full tests require a mock ProcessManager, which is complex to set up.
    // The real integration testing happens in the contract tests for gglib-proxy.
}
