//! Model runtime port for proxy model management.
//!
//! This port defines the interface for ensuring a model is running
//! and ready to serve requests. It abstracts the process management
//! details from the proxy layer.

use async_trait::async_trait;
use std::fmt;
use thiserror::Error;

use crate::cache_config::CacheRamSetting;
use crate::domain::CacheRamHealth;
use crate::ports::ProcessHandle;
use crate::server_config::ServerConfigOptions;

/// Per-call launch overrides layered on a runtime's standing configuration.
///
/// A runtime is normally built once with a standing template — the proxy's
/// cache settings, say — and then shared, so that only one llama-server runs
/// at a time. This is how an individual caller contributes launch options on
/// top of that template without needing a manager of its own.
///
/// `Default` means "no opinion": every field falls through to the template.
#[derive(Debug, Clone, Default)]
pub struct LaunchOverrides {
    /// Explicit options merged over the runtime's template, `Some` fields
    /// winning — see [`ServerConfigOptions::overlay`].
    pub options: ServerConfigOptions,
    /// How to size llama-server's host-RAM prompt cache for this launch.
    ///
    /// Separate from [`Self::options`] because it is resolved at spawn against
    /// live system RAM and the model's KV footprint, not carried as a flag.
    /// `None` defers to the runtime's own setting.
    pub cache_ram: Option<CacheRamSetting>,
}

/// Target information for a running model instance.
///
/// This struct contains all information needed to route requests
/// to a running llama-server instance.
#[derive(Debug, Clone)]
pub struct RunningTarget {
    /// Full URL to the server (e.g., <http://127.0.0.1:5500>).
    /// Future-proof for non-localhost deployments.
    pub base_url: String,
    /// Port the server is listening on.
    pub port: u16,
    /// Database ID of the model.
    pub model_id: u32,
    /// Human-readable model name (for logging/headers).
    pub model_name: String,
    /// Actual context size being used.
    pub effective_ctx: u64,
    /// True when this instance was freshly spawned (restart or cold start).
    pub just_started: bool,
    /// Whether llama-server's disk slot save/restore can actually resume this
    /// model, i.e. its KV memory retains the full token history.
    ///
    /// False for sliding-window, hybrid, and recurrent architectures (see
    /// [`crate::domain::kv_memory_is_partial`]): the slot file carries KV
    /// state and tokens but not the server's context checkpoints, so a
    /// restore leaves the slot unable to resume and llama-server re-prefills
    /// the whole prompt. Callers skip the disk slot layer when this is false
    /// and let the in-RAM prompt cache — which does keep checkpoints — handle
    /// conversation switching.
    pub slot_restore_supported: bool,
    /// How healthy the host-RAM prompt cache budget (`--cache-ram`) resolved
    /// for this launch is.
    ///
    /// Classified once at spawn (where the budget arithmetic and the
    /// auto-vs-explicit distinction are both in scope) and carried here so
    /// user-facing surfaces can report it without re-deriving thresholds. See
    /// [`crate::domain::classify_cache_ram`].
    pub cache_ram_health: CacheRamHealth,
}

impl RunningTarget {
    /// Create a new `RunningTarget` for a local server.
    ///
    /// `slot_restore_supported` defaults to `true` (the full-attention case)
    /// and `cache_ram_health` to [`CacheRamHealth::LlamaDefault`] (no flag
    /// emitted); callers that know the launch's actual resolution narrow them
    /// with [`Self::with_slot_restore_supported`] and
    /// [`Self::with_cache_ram_health`].
    #[must_use]
    pub fn local(
        port: u16,
        model_id: u32,
        model_name: String,
        effective_ctx: u64,
        just_started: bool,
    ) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            port,
            model_id,
            model_name,
            effective_ctx,
            just_started,
            slot_restore_supported: true,
            cache_ram_health: CacheRamHealth::LlamaDefault,
        }
    }

    /// Set whether disk slot restore can resume this model.
    #[must_use]
    pub const fn with_slot_restore_supported(mut self, supported: bool) -> Self {
        self.slot_restore_supported = supported;
        self
    }

    /// Set the resolved host-RAM prompt cache health for this launch.
    #[must_use]
    pub const fn with_cache_ram_health(mut self, health: CacheRamHealth) -> Self {
        self.cache_ram_health = health;
        self
    }
}

/// Errors that can occur during model runtime operations.
#[derive(Clone, Debug, Error)]
pub enum ModelRuntimeError {
    /// The requested model was not found in the catalog.
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// A model is currently loading; try again later.
    /// Callers should return 503 Service Unavailable.
    #[error("Model is loading, try again")]
    ModelLoading,

    /// Retryable: another caller is loading the same model, we waited too long for contention to clear.
    #[error("Contention timeout: {0}")]
    ContentionTimeout(String),

    /// Failed to spawn the model server process.
    #[error("Failed to start model: {0}")]
    SpawnFailed(String),

    /// The model server failed its health check.
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    /// The model file was not found on disk.
    #[error("Model file not found: {0}")]
    ModelFileNotFound(String),

    /// A model other than the pinned one was requested.
    ///
    /// Only reachable in pinned mode (`gglib serve <model>`), which exists to
    /// give single-model clients — VS Code Copilot's BYOK endpoint, for one —
    /// an endpoint that never switches models underneath them. Swapping to the
    /// requested model would defeat that guarantee, so the request is refused
    /// rather than served.
    #[error("Server is pinned to model '{expected}'; refusing request for '{requested}'")]
    PinnedModelMismatch {
        /// The model this server was pinned to at startup.
        expected: String,
        /// The model the caller asked for.
        requested: String,
    },

    /// Internal error during runtime operations.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl ModelRuntimeError {
    /// Returns true if this error indicates a temporary condition
    /// where retrying may succeed.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::ModelLoading | Self::ContentionTimeout(_))
    }

    /// Returns a suggested HTTP status code for this error.
    #[must_use]
    pub const fn suggested_status_code(&self) -> u16 {
        match self {
            Self::ModelLoading | Self::ContentionTimeout(_) => 503,
            // A pinned mismatch is 404, not 403: from the client's point of
            // view the model it asked for does not exist on this endpoint.
            Self::ModelNotFound(_)
            | Self::ModelFileNotFound(_)
            | Self::PinnedModelMismatch { .. } => 404,
            Self::SpawnFailed(_) | Self::HealthCheckFailed(_) | Self::Internal(_) => 500,
        }
    }
}

/// Port for managing model runtime (ensuring models are running).
///
/// This is the primary interface the proxy uses to get a running
/// model server. Implementations handle:
/// - Model resolution (name → file path)
/// - Process lifecycle (start, stop, health check)
/// - Context size management
/// - Single-swap or concurrent strategies
#[async_trait]
pub trait ModelRuntimePort: Send + Sync + fmt::Debug {
    /// Ensure a model is running and ready to serve requests.
    ///
    /// This method:
    /// 1. Resolves the model name to a database entry
    /// 2. Checks if the model is already running with the correct context
    /// 3. Starts or restarts the model if needed
    /// 4. Waits for the health check to pass
    /// 5. Returns the target information for routing
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name or alias of the model to run
    /// * `num_ctx` - Optional context size override from request
    /// * `default_ctx` - Default context size if not specified
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started.
    async fn ensure_model_running(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError>;

    /// Same as [`Self::ensure_model_running`], but with per-call overrides
    /// layered on top of whatever standing configuration the implementation
    /// was built with.
    ///
    /// Lets one shared runtime — and therefore one llama-server at a time —
    /// serve callers with different launch needs, instead of each caller
    /// constructing its own manager and losing that guarantee. A GUI start
    /// request carrying `--mlock` and a benchmark run that must never gain a
    /// prompt cache can both go through the same instance.
    ///
    /// Defaults to ignoring the overrides and delegating, so implementations
    /// with no per-call configuration to apply need not override it.
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started.
    async fn ensure_model_running_with(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
        overrides: LaunchOverrides,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        let _ = overrides;
        self.ensure_model_running(model_name, num_ctx, default_ctx)
            .await
    }

    /// Get information about the currently running model, if any.
    ///
    /// Returns `None` if no model is currently running.
    async fn current_model(&self) -> Option<RunningTarget>;

    /// Every llama-server process this runtime currently owns.
    ///
    /// Sibling of [`Self::current_model`] for callers that need process-level
    /// detail — pid and start time — rather than routing information; the GUI
    /// server list is the motivating case.
    ///
    /// Defaults to empty for runtimes that do not track individual processes
    /// (test doubles, remote backends). Returning nothing is always safe here:
    /// callers treat it as "no servers to show".
    async fn list_running(&self) -> Vec<ProcessHandle> {
        Vec::new()
    }

    /// Stop the currently running model.
    ///
    /// This is primarily for cleanup/shutdown scenarios.
    async fn stop_current(&self) -> Result<(), ModelRuntimeError>;

    /// The one model this runtime is pinned to, if any.
    ///
    /// `Some(name)` means every other model is refused with
    /// [`ModelRuntimeError::PinnedModelMismatch`] rather than swapped to —
    /// the mode `gglib serve` runs in. `None` is the ordinary auto-swapping
    /// runtime.
    ///
    /// Synchronous because pinning is fixed when the runtime is constructed
    /// and never changes afterwards, unlike [`Self::current_model`], which
    /// reports live process state.
    ///
    /// Defaults to unpinned so test doubles and remote backends need not
    /// implement it. Callers use it to avoid offering a model that would only
    /// be refused — `/v1/models` being the motivating case.
    fn pinned_model(&self) -> Option<&str> {
        None
    }
}

/// A [`ModelRuntimePort`] that never has anything running.
///
/// For callers with no shared [`ProcessManager`](crate::ports::ProcessRunner)
/// to point at — the CLI's single-shot commands, whose `is_serving` checks
/// against a runtime scoped to that one process invocation would report
/// "nothing running" regardless, since nothing was started in it. Making that
/// explicit here is more honest than wiring in a real runner that can only
/// ever agree.
#[derive(Debug, Default)]
pub struct NoopModelRuntime;

#[async_trait]
impl ModelRuntimePort for NoopModelRuntime {
    async fn ensure_model_running(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Err(ModelRuntimeError::Internal(
            "no runtime available in this context".to_string(),
        ))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Implements only the three required methods, so the defaulted ones are
    /// exercised exactly as an untouched test double would get them.
    #[derive(Debug)]
    struct MinimalRuntime;

    #[async_trait]
    impl ModelRuntimePort for MinimalRuntime {
        async fn ensure_model_running(
            &self,
            model_name: &str,
            num_ctx: Option<u64>,
            default_ctx: u64,
        ) -> Result<RunningTarget, ModelRuntimeError> {
            Ok(RunningTarget::local(
                5500,
                1,
                model_name.to_string(),
                num_ctx.unwrap_or(default_ctx),
                false,
            ))
        }

        async fn current_model(&self) -> Option<RunningTarget> {
            None
        }

        async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
            Ok(())
        }
    }

    /// The default must delegate rather than fail, which is what lets existing
    /// implementations adopt the trait change without being edited.
    #[tokio::test]
    async fn ensure_model_running_with_defaults_to_delegating() {
        let target = MinimalRuntime
            .ensure_model_running_with("m", Some(8192), 4096, LaunchOverrides::default())
            .await
            .expect("default implementation should delegate");

        assert_eq!(target.model_name, "m");
        assert_eq!(target.effective_ctx, 8192);
    }

    /// Overrides are dropped by the default, not silently half-applied — an
    /// implementation that cares must opt in by overriding the method.
    #[tokio::test]
    async fn default_ensure_model_running_with_ignores_overrides() {
        let overrides = LaunchOverrides {
            options: ServerConfigOptions {
                context_size: Some(999),
                ..Default::default()
            },
            cache_ram: Some(CacheRamSetting::ExplicitMb(0)),
        };

        let target = MinimalRuntime
            .ensure_model_running_with("m", None, 4096, overrides)
            .await
            .unwrap();

        assert_eq!(target.effective_ctx, 4096);
    }

    /// Unpinned is the safe default: a runtime that says nothing about
    /// pinning must not cause callers to narrow what they offer.
    #[test]
    fn pinned_model_defaults_to_unpinned() {
        assert_eq!(MinimalRuntime.pinned_model(), None);
    }

    #[tokio::test]
    async fn list_running_defaults_to_empty() {
        assert!(MinimalRuntime.list_running().await.is_empty());
    }

    /// "No opinion" has to be the default, or merging one in would silently
    /// override the runtime's own template.
    #[test]
    fn launch_overrides_default_is_empty() {
        let overrides = LaunchOverrides::default();
        assert!(overrides.cache_ram.is_none());
        assert!(overrides.options.context_size.is_none());
        assert!(overrides.options.mlock.is_none());
    }

    fn pinned_mismatch() -> ModelRuntimeError {
        ModelRuntimeError::PinnedModelMismatch {
            expected: "qwen2.5".to_string(),
            requested: "llama-3-8b".to_string(),
        }
    }

    /// 404 rather than 403: from the caller's point of view the model it asked
    /// for does not exist on this endpoint.
    #[test]
    fn pinned_mismatch_is_not_found() {
        assert_eq!(pinned_mismatch().suggested_status_code(), 404);
    }

    /// Retrying the identical request can never succeed — the pin is fixed for
    /// the process lifetime — so clients must not back off and retry.
    #[test]
    fn pinned_mismatch_is_not_retryable() {
        assert!(!pinned_mismatch().is_retryable());
    }

    /// Both model names belong in the message; without them the caller cannot
    /// tell what this endpoint actually serves.
    #[test]
    fn pinned_mismatch_names_both_models() {
        let rendered = pinned_mismatch().to_string();
        assert!(rendered.contains("qwen2.5"), "{rendered}");
        assert!(rendered.contains("llama-3-8b"), "{rendered}");
    }
}
