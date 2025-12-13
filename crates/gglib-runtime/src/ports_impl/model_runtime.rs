//! ModelRuntimePort implementation using ProcessManager.
//!
//! This adapter wraps the ProcessManager (SingleSwap strategy) to implement
//! the ModelRuntimePort interface from gglib-core.

use async_trait::async_trait;
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
}

impl RuntimePortImpl {
    /// Create a new RuntimePortImpl.
    ///
    /// # Arguments
    ///
    /// * `mgr` - ProcessManager configured with SingleSwap strategy
    pub fn new(mgr: Arc<ProcessManager>) -> Self {
        Self { mgr }
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
            .ensure_model_running(model_name, num_ctx, default_ctx)
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
