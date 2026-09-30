//! Unified process manager for llama-server instances.
//!
//! Every launch surface — the CLI, the proxy, both GUIs — shares one manager
//! (built once by `build_service_graph`), which is what makes "gglib owns every
//! llama-server on this machine" an invariant rather than a hope.
//!
//! It keeps a bounded resident set — see
//! [`admission`](crate::process::admission) for how many, and why — with a
//! queue deciding who occupies it. This type routes, and [`ResidentSet`] owns
//! both the state and the launch sequence that mutates it.

use super::core::GuiProcessCore;
use anyhow::Result;
use gglib_core::domain::AdmissionSnapshot;
use gglib_core::ports::{
    Admission, AdmissionLease, LaunchOverrides, ModelCatalogPort, ModelRuntimeError, ProcessHandle,
    RunningTarget,
};
use gglib_core::server_config::{CacheRamSetting, ServerConfigOptions};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::process::residency::ResidentSet;

/// Unified process manager for llama-server instances.
///
/// Wraps a [`ResidentSet`] — the VRAM slots and the admission queue that fills
/// them — with the process-level queries the GUI and CLI need.
pub struct ProcessManager {
    core: Arc<RwLock<GuiProcessCore>>,
    residency: ResidentSet,
}

impl ProcessManager {
    /// Create a new `ProcessManager`.
    ///
    /// Requests are admitted through a queue rather than racing each other: a
    /// request for a model that is already resident is served immediately,
    /// while one for a model that is not waits until it can take a VRAM slot —
    /// either alongside what is loaded, or by displacing it once nothing is
    /// being served from it. See [`crate::process::admission`].
    ///
    /// # Arguments
    ///
    /// * `base_port` — Base port for llama-server allocation. Ports are
    ///   assigned sequentially starting from this value.
    /// * `llama_server_path` — Path to the llama-server binary to execute.
    /// * `catalog` — Model catalog used to resolve model names into launch
    ///   specifications (file paths, context sizes, etc.).
    /// * `launch_overrides` — Standing launch options every spawn starts from
    ///   (slot-save path, cache reuse, KV cache types, and anything else
    ///   [`ServerConfigOptions`] carries). Per-call [`LaunchOverrides`] are
    ///   layered on top; see [`ResidentSet`] for the composition order.
    /// * `cache_ram` — how to size llama-server's own host-RAM prompt cache
    ///   (`--cache-ram`). Not part of `launch_overrides` because it is resolved
    ///   at spawn rather than passed through.
    ///   [`CacheRamSetting::ExplicitMb`]`(0)` disables the cache outright — the
    ///   right choice for benchmark launches, where a prompt cache would
    ///   perturb prefill timings. (Omitting the flag entirely, so llama-server's
    ///   own default applies, is what [`CacheRamSetting::Auto`] does when
    ///   autosizing is suppressed by env.)
    ///
    /// Use [`Self::set_pin`] afterwards when the manager must refuse every
    /// model but one (`gglib serve`).
    pub fn new(
        base_port: u16,
        llama_server_path: impl Into<String>,
        catalog: Arc<dyn ModelCatalogPort>,
        launch_overrides: ServerConfigOptions,
        cache_ram: CacheRamSetting,
    ) -> Self {
        let core = GuiProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            residency: ResidentSet::new(catalog, launch_overrides, cache_ram),
        }
    }

    /// Pin this manager to a single model, or clear the pin (`gglib serve`).
    ///
    /// While pinned, the manager behaves exactly like the unpinned one for the
    /// pinned model — same admission, same cache handling, same launch options
    /// template, with the pin's own overrides layered on top — but rejects
    /// every other model with [`ModelRuntimeError::PinnedModelMismatch`]
    /// instead of admitting it.
    ///
    /// That refusal is the feature. `gglib serve <model>` exists to give
    /// single-model clients (VS Code Copilot's BYOK endpoint, for one) an
    /// endpoint that cannot change model underneath them; silently honouring a
    /// foreign request would defeat the guarantee they are relying on.
    ///
    /// Runtime-mutable rather than a constructor because the daemon owns one
    /// long-lived manager: the pin is applied when a pinned proxy run starts
    /// and cleared when it stops.
    pub fn set_pin(&self, pin: Option<gglib_core::ports::PinnedSpec>) {
        self.residency.set_pin(pin);
    }

    /// Admit a request to a running model.
    ///
    /// The returned [`Admission::lease`] must be held for as long as the
    /// request is being served — it is what tells the queue the model is still
    /// in use and must not be swapped out. See
    /// [`ModelRuntimePort::admit`](gglib_core::ports::ModelRuntimePort::admit).
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started, or
    /// [`ModelRuntimeError::AdmissionTimeout`] if the request never reached the
    /// front of the queue.
    ///
    /// # Known limitations
    ///
    /// If a displaced model's shutdown timed out (D-state process), the
    /// subsequent spawn may fail with a port-in-use or CUDA OOM error. There is
    /// no automatic retry — the caller receives the error and must retry
    /// manually.
    pub async fn admit(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: Option<u64>,
        overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.residency
            .admit(&self.core, model_name, num_ctx, default_ctx, overrides)
            .await
    }

    /// What the admission queue and resident set look like right now.
    #[must_use]
    pub fn admission_snapshot(&self) -> AdmissionSnapshot {
        self.residency.queue().snapshot()
    }

    /// Get information about the model in the primary slot.
    ///
    /// The primary is the slot chat traffic follows; a co-resident auxiliary
    /// model is deliberately not reported here, because every caller of this
    /// method — the `/slots` poller, the GUI's running-model panel — means "the
    /// model this endpoint is serving".
    pub fn current_model(&self) -> Option<RunningTarget> {
        self.residency.current_model()
    }

    /// Stop the model in the primary slot.
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the process could not be stopped.
    pub async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.residency.stop_primary(&self.core).await
    }

    /// List running servers as [`ProcessHandle`]s.
    ///
    /// The core's own records, projected onto the port type so callers that
    /// already speak `ProcessHandle` can consume a manager-backed runtime
    /// without a second shape to handle.
    pub async fn list_running(&self) -> Vec<ProcessHandle> {
        let core = self.core.read().await;
        core.list_all()
            .into_iter()
            .map(|info| {
                ProcessHandle::new(
                    i64::from(info.model_id),
                    info.model_name.clone(),
                    Some(info.pid),
                    info.port,
                    info.started_at,
                )
            })
            .collect()
    }

    /// The single model this manager is pinned to, if any.
    ///
    /// `Some(name)` is `gglib serve`: every other model is refused rather
    /// than admitted. Owned because the pin is runtime-mutable state behind a
    /// lock (see [`Self::set_pin`]).
    #[must_use]
    pub fn pinned_model(&self) -> Option<String> {
        self.residency.pinned_name()
    }

    /// Keep model `model_id` on `port` loaded, and unrecycled, until the
    /// returned lease drops. See
    /// [`ModelRuntimePort::hold`](gglib_core::ports::ModelRuntimePort::hold).
    #[must_use]
    pub fn hold(&self, port: u16, model_id: u32) -> Option<AdmissionLease> {
        self.residency.queue().hold(port, model_id)
    }

    /// Check if any slot is mid-launch.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.residency.queue().is_loading()
    }
}

// Note: ProcessManager is not Clone because ResidentSet contains
// Arc<dyn ...> and RwLock which don't trivially clone in a meaningful way.
// If you need shared access, wrap ProcessManager in Arc.

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
