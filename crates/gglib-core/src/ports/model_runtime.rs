//! Model runtime port for proxy model management.
//!
//! This port defines the interface for admitting a request to a running model.
//! It abstracts the process management details from the proxy layer.
//!
//! ## Admission, not "ensure running"
//!
//! The entry point is [`ModelRuntimePort::admit`], and it returns an
//! [`Admission`] — a routing target *plus a lease*. The lease is what makes
//! request batching possible: the runtime cannot decide whether it is safe to
//! swap models unless it knows how many requests are still being served by the
//! one currently loaded. Holding the lease for the life of the request is
//! therefore not bookkeeping, it is the mechanism.
//!
//! A caller that only wants a model up and does not care when it goes away
//! (the GUI's "start model" button) drops the lease immediately; the model
//! stays resident until something else wins admission.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

use crate::cache_config::CacheRamSetting;
use crate::domain::{AdmissionSnapshot, CacheRamHealth, LaunchNarration, ModelSamplingDefaults};
use crate::ports::ProcessHandle;
use crate::server_config::ServerConfigOptions;

/// Per-call launch overrides layered on a runtime's standing configuration.
///
/// A runtime is normally built once with a standing template — the proxy's
/// cache settings, say — and then shared, so that one admission queue governs
/// every llama-server on the machine. This is how an individual caller
/// contributes launch options on top of that template without needing a manager
/// of its own.
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
    /// What this launch decided, and why (see
    /// [`crate::domain::LaunchNarration`]).
    ///
    /// Carried on the target for the same reason as
    /// [`Self::cache_ram_health`]: the resolutions and their provenance exist
    /// only at spawn, so anything downstream that wants to explain the
    /// running model has no way to recover them otherwise. `None` for targets
    /// that did not come from a gglib launch.
    pub narration: Option<LaunchNarration>,
    /// What this model's own GGUF declares about sampler defaults.
    ///
    /// `None` for targets that did not come from a gglib launch, in the same
    /// sense as [`Self::narration`] — nobody read a GGUF for them, so nothing
    /// is known either way. Distinct from `Some(ModelSamplingDefaults::default())`,
    /// which is the ordinary case: a GGUF was read and it declares nothing.
    ///
    /// Consumers must not flatten those two. `None` means the model's
    /// contribution to `/props` is unknown and no field can be attributed;
    /// `Some(default())` means the build's own table is showing through
    /// unmodified. See [`crate::domain::ModelSamplingDefaults`].
    pub model_sampling: Option<ModelSamplingDefaults>,
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
            narration: None,
            model_sampling: None,
        }
    }

    /// Attach what the launched model's GGUF declares about sampling.
    #[must_use]
    pub const fn with_model_sampling(mut self, declared: ModelSamplingDefaults) -> Self {
        self.model_sampling = Some(declared);
        self
    }

    /// Attach the narration of the launch that produced this target.
    #[must_use]
    pub fn with_narration(mut self, narration: LaunchNarration) -> Self {
        self.narration = Some(narration);
        self
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

/// The runtime side of an [`AdmissionLease`]: what to call when a request that
/// was holding a VRAM slot is finished with it.
///
/// A separate trait rather than a closure so the lease stays `Debug` and has no
/// generic parameter to thread through every signature that carries one. The
/// implementation lives in `gglib-runtime`; this crate only needs to be able to
/// call it from a `Drop`, which is why [`Self::release`] is synchronous and
/// must not block.
pub trait AdmissionRelease: Send + Sync + fmt::Debug {
    /// Release one in-flight reference to `slot`, waking the scheduler if that
    /// was the last one.
    ///
    /// Called from [`AdmissionLease`]'s `Drop`, so it must never block, panic,
    /// or await.
    fn release(&self, slot: usize);
}

/// Proof that a request is being served by a resident model, and that the
/// runtime must not evict that model until the request is done.
///
/// Dropping the lease releases the slot. Every exit path a request has — normal
/// completion, `?`, client disconnect, panic unwind — runs `Drop`, so there is
/// no path that leaks a reference and wedges the scheduler. This is the same
/// guarantee, for the same reason, that the proxy's connection registry gets
/// from its own guard.
///
/// Not `Clone`: two owners would mean two releases for one acquisition.
#[derive(Debug)]
pub struct AdmissionLease {
    owner: Option<Arc<dyn AdmissionRelease>>,
    slot: usize,
}

impl AdmissionLease {
    /// Create a lease that releases `slot` on `owner` when dropped.
    #[must_use]
    pub fn new(owner: Arc<dyn AdmissionRelease>, slot: usize) -> Self {
        Self {
            owner: Some(owner),
            slot,
        }
    }

    /// A lease that owns nothing and releases nothing.
    ///
    /// For runtimes with no resident set to account for — test doubles and the
    /// [`NoopModelRuntime`] — so they are not forced to implement a scheduler
    /// to satisfy the signature.
    #[must_use]
    pub const fn detached() -> Self {
        Self {
            owner: None,
            slot: 0,
        }
    }

    /// Which resident slot this lease is holding.
    #[must_use]
    pub const fn slot(&self) -> usize {
        self.slot
    }
}

impl Drop for AdmissionLease {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            owner.release(self.slot);
        }
    }
}

/// A granted admission: where to send the request, and the lease that keeps the
/// model loaded while it is in flight.
///
/// The two are returned together rather than the lease being attached to
/// [`RunningTarget`] because the target must stay `Clone` — the startup guard
/// broadcasts one target to every caller waiting on the same launch — and a
/// clonable lease would release once per clone.
#[derive(Debug)]
pub struct Admission {
    /// Where to route the request.
    pub target: RunningTarget,
    /// Held for the life of the request. See [`AdmissionLease`].
    pub lease: AdmissionLease,
}

impl Admission {
    /// An admission with no slot accounting, for runtimes that do not have any.
    #[must_use]
    pub const fn detached(target: RunningTarget) -> Self {
        Self {
            target,
            lease: AdmissionLease::detached(),
        }
    }

    /// Take the target and drop the lease immediately.
    ///
    /// For callers that want a model launched but have no request to hold it
    /// for — `gglib model start` and the GUI's start button. The model stays
    /// resident; it is simply evictable from this moment on.
    #[must_use]
    pub fn into_target(self) -> RunningTarget {
        self.target
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

    /// Retryable: the request sat in the admission queue past its deadline
    /// without ever reaching the front.
    ///
    /// Reaching this means the GPU stayed continuously occupied by other models
    /// for longer than a request can reasonably wait — not that a collision was
    /// mishandled. The queue's own fairness bounds make it rare; when it does
    /// happen the caller gets a 503 with `Retry-After` and control of its own
    /// backoff.
    #[error("Admission timeout: {0}")]
    AdmissionTimeout(String),

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
        matches!(self, Self::ModelLoading | Self::AdmissionTimeout(_))
    }

    /// Returns a suggested HTTP status code for this error.
    #[must_use]
    pub const fn suggested_status_code(&self) -> u16 {
        match self {
            Self::ModelLoading | Self::AdmissionTimeout(_) => 503,
            // A pinned mismatch is 404, not 403: from the client's point of
            // view the model it asked for does not exist on this endpoint.
            Self::ModelNotFound(_)
            | Self::ModelFileNotFound(_)
            | Self::PinnedModelMismatch { .. } => 404,
            Self::SpawnFailed(_) | Self::HealthCheckFailed(_) | Self::Internal(_) => 500,
        }
    }
}

/// Canonical `error.type` discriminants, shared by every surface.
///
/// `gglib_proxy::models::ErrorResponse` carries one of these over HTTP and
/// [`RuntimeErrorEnvelope`] carries the same vocabulary over Tauri IPC and SSE,
/// so a client that learns it once understands both.
pub mod error_type {
    /// Transient unavailability — the same request may succeed if retried.
    pub const SERVICE_UNAVAILABLE: &str = "service_unavailable";
    /// The caller asked for something that does not exist or is not permitted.
    pub const INVALID_REQUEST: &str = "invalid_request_error";
    /// The server failed in a way that retrying will not fix.
    pub const SERVER_ERROR: &str = "server_error";
}

/// Whether a wire `error.type` discriminant denotes a retryable condition.
///
/// The single definition of retryability keyed on the wire vocabulary. An HTTP
/// client parsing an error body and an IPC consumer reading a
/// [`RuntimeErrorEnvelope`] both route through here, so the two cannot drift
/// into disagreeing about what is worth retrying.
#[must_use]
pub fn is_retryable_error_type(discriminant: &str) -> bool {
    discriminant == error_type::SERVICE_UNAVAILABLE
}

/// Structured, serializable view of a [`ModelRuntimeError`].
///
/// For IPC boundaries (Tauri events, SSE) that need machine-readable type +
/// retry hints alongside the human-readable message, mirroring the shape
/// `gglib_proxy::models::ErrorResponse` already sends over HTTP.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeErrorEnvelope {
    /// Human-readable error message.
    pub message: String,
    /// Stable error type discriminant, matching the `type` strings the HTTP
    /// layer already sends for the same `ModelRuntimeError` variants (e.g.
    /// `"service_unavailable"`), so GUI and HTTP clients agree on meaning.
    pub r#type: String,
    /// Whether retrying the same request may succeed.
    pub retryable: bool,
}

impl From<&ModelRuntimeError> for RuntimeErrorEnvelope {
    fn from(err: &ModelRuntimeError) -> Self {
        let discriminant = match err {
            ModelRuntimeError::ModelLoading | ModelRuntimeError::AdmissionTimeout(_) => {
                error_type::SERVICE_UNAVAILABLE
            }
            ModelRuntimeError::ModelNotFound(_)
            | ModelRuntimeError::ModelFileNotFound(_)
            | ModelRuntimeError::PinnedModelMismatch { .. } => error_type::INVALID_REQUEST,
            ModelRuntimeError::SpawnFailed(_)
            | ModelRuntimeError::HealthCheckFailed(_)
            | ModelRuntimeError::Internal(_) => error_type::SERVER_ERROR,
        };
        Self {
            message: err.to_string(),
            r#type: discriminant.to_string(),
            retryable: err.is_retryable(),
        }
    }
}

/// Port for admitting requests to a running model.
///
/// This is the primary interface the proxy uses to get a running
/// model server. Implementations handle:
/// - Model resolution (name → file path)
/// - Process lifecycle (start, stop, health check)
/// - Context size management
/// - Admission control: queueing, batching, and the VRAM resident set
#[async_trait]
pub trait ModelRuntimePort: Send + Sync + fmt::Debug {
    /// Admit a request to a running model, launching or swapping if needed.
    ///
    /// This method:
    /// 1. Resolves the model name to a database entry
    /// 2. Admits immediately if the model is already resident
    /// 3. Otherwise queues until the model can take a VRAM slot — either by
    ///    co-loading alongside what is already there, or by swapping once the
    ///    outgoing model has no requests left in flight
    /// 4. Waits for the health check to pass
    /// 5. Returns the routing target and a lease on the slot
    ///
    /// **The returned [`Admission::lease`] must be held for as long as the
    /// request is being served.** Dropping it early tells the runtime the slot
    /// is free and permits a swap out from under a live generation. Callers
    /// that only want the model launched — not served — use
    /// [`Admission::into_target`], which drops the lease deliberately.
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name or alias of the model to run
    /// * `num_ctx` - Optional context size override from request
    /// * `default_ctx` - Default context size if not specified
    /// * `overrides` - Per-call launch options layered on the runtime's
    ///   standing template, so one shared runtime can serve callers with
    ///   different launch needs (a GUI start carrying `--mlock`, a benchmark
    ///   that must never gain a prompt cache). [`LaunchOverrides::default`]
    ///   means "no opinion".
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started, or
    /// [`ModelRuntimeError::AdmissionTimeout`] if the request never reached
    /// the front of the queue.
    async fn admit(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
        overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError>;

    /// What the admission queue and the VRAM resident set look like right now.
    ///
    /// Synchronous for the same reason [`Self::pinned_model`] is: it is a
    /// single read of plain shared state, not a query against live process
    /// state. The dashboard publisher calls it on every tick.
    ///
    /// Defaults to empty for runtimes with no resident set to report (test
    /// doubles, remote backends).
    fn admission_snapshot(&self) -> AdmissionSnapshot {
        AdmissionSnapshot::default()
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
    /// Synchronous because the pin is plain shared state, unlike
    /// [`Self::current_model`], which reports live process state. Owned
    /// rather than borrowed because the pin can change at runtime (see
    /// [`Self::set_pin`]) — a borrow could not outlive the lock guarding it.
    ///
    /// Defaults to unpinned so test doubles and remote backends need not
    /// implement it. Callers use it to avoid offering a model that would only
    /// be refused — `/v1/models` being the motivating case.
    fn pinned_model(&self) -> Option<String> {
        None
    }

    /// Pin this runtime to a single model, or clear the pin.
    ///
    /// `Some(spec)` makes every request for another model fail with
    /// [`ModelRuntimeError::PinnedModelMismatch`] instead of swapping; the
    /// spec's launch overrides are layered onto the runtime's standing
    /// template for the pinned model's launches. `None` restores ordinary
    /// auto-swapping. This is how `gglib serve` reaches the daemon's shared
    /// runtime: the pin travels over `POST /api/proxy/start` rather than
    /// being fixed at construction.
    ///
    /// # Errors
    ///
    /// The default refuses, so a runtime that cannot honour a pin (test
    /// doubles, remote backends) fails loudly instead of silently serving
    /// every model against the caller's explicit instruction.
    fn set_pin(&self, pin: Option<PinnedSpec>) -> Result<(), ModelRuntimeError> {
        let _ = pin;
        Err(ModelRuntimeError::Internal(
            "this runtime does not support pinning".to_string(),
        ))
    }
}

/// A runtime pin: the one model a runtime will serve, plus how to launch it.
///
/// Carried by [`ModelRuntimePort::set_pin`] and serialized inside the
/// daemon's `POST /api/proxy/start` body, which is why it derives serde.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PinnedSpec {
    /// Name clients must address the model by. Matched exactly.
    pub name: String,
    /// Standing launch options for the pinned model, already resolved
    /// through the caller's cascade — layered onto the runtime's template at
    /// launch, winning field-wise (the cascade has already run; the template
    /// must not undo it).
    #[serde(default)]
    pub launch_overrides: ServerConfigOptions,
}

/// A [`ModelRuntimePort`] that never has anything running.
///
/// For callers with no shared `ProcessManager` to point at — the CLI's
/// single-shot commands, whose `is_serving` checks
/// against a runtime scoped to that one process invocation would report
/// "nothing running" regardless, since nothing was started in it. Making that
/// explicit here is more honest than wiring in a real runner that can only
/// ever agree.
#[derive(Debug, Default)]
pub struct NoopModelRuntime;

#[async_trait]
impl ModelRuntimePort for NoopModelRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
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
        async fn admit(
            &self,
            model_name: &str,
            num_ctx: Option<u64>,
            default_ctx: u64,
            _overrides: LaunchOverrides,
        ) -> Result<Admission, ModelRuntimeError> {
            Ok(Admission::detached(RunningTarget::local(
                5500,
                1,
                model_name.to_string(),
                num_ctx.unwrap_or(default_ctx),
                false,
            )))
        }

        async fn current_model(&self) -> Option<RunningTarget> {
            None
        }

        async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
            Ok(())
        }
    }

    /// A runtime with no resident set to account for must still hand back a
    /// usable lease, or every test double would need a scheduler.
    #[tokio::test]
    async fn a_minimal_runtime_admits_with_a_detached_lease() {
        let admission = MinimalRuntime
            .admit("m", Some(8192), 4096, LaunchOverrides::default())
            .await
            .expect("minimal runtime admits");

        assert_eq!(admission.target.model_name, "m");
        assert_eq!(admission.target.effective_ctx, 8192);
        assert_eq!(admission.lease.slot(), 0);
        // Dropping it must be a no-op rather than a panic.
        drop(admission);
    }

    #[test]
    fn admission_snapshot_defaults_to_empty() {
        let snapshot = MinimalRuntime.admission_snapshot();
        assert!(snapshot.slots.is_empty());
        assert!(snapshot.queued.is_empty());
        assert_eq!(snapshot.total_swaps, 0);
    }

    /// The whole point of the lease: exactly one release per acquisition, on
    /// every exit path including an unwinding panic.
    #[test]
    fn a_lease_releases_its_slot_exactly_once_on_drop() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Debug, Default)]
        struct Counter(AtomicUsize);

        impl AdmissionRelease for Counter {
            fn release(&self, slot: usize) {
                assert_eq!(slot, 1, "the lease must release the slot it was given");
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(Counter::default());
        {
            let lease = AdmissionLease::new(Arc::clone(&counter) as Arc<dyn AdmissionRelease>, 1);
            assert_eq!(lease.slot(), 1);
            assert_eq!(counter.0.load(Ordering::SeqCst), 0, "not yet released");
        }
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);

        // A panic unwinds through Drop just the same — this workspace sets no
        // `panic = "abort"` profile, so a panicking handler cannot leak a slot.
        let counter2 = Arc::new(Counter::default());
        let held = Arc::clone(&counter2);
        let result = std::panic::catch_unwind(move || {
            let _lease = AdmissionLease::new(held as Arc<dyn AdmissionRelease>, 1);
            panic!("handler blew up mid-request");
        });
        assert!(result.is_err());
        assert_eq!(counter2.0.load(Ordering::SeqCst), 1, "released on unwind");
    }

    /// A detached lease has nothing to release, so dropping it must not reach
    /// for an owner that is not there.
    #[test]
    fn a_detached_lease_drops_cleanly() {
        let lease = AdmissionLease::detached();
        assert_eq!(lease.slot(), 0);
        drop(lease);
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

    /// A retryable error's envelope must carry `retryable: true` and the
    /// `service_unavailable` type, matching the HTTP layer's 503 mapping.
    #[test]
    fn envelope_for_admission_timeout_is_retryable_service_unavailable() {
        let err = ModelRuntimeError::AdmissionTimeout("waited too long".to_string());
        let envelope = RuntimeErrorEnvelope::from(&err);
        assert_eq!(envelope.r#type, "service_unavailable");
        assert!(envelope.retryable);
        assert_eq!(envelope.message, err.to_string());
    }

    /// A non-retryable error's envelope must say so, matching the HTTP
    /// layer's non-503 mapping.
    #[test]
    fn envelope_for_pinned_mismatch_is_not_retryable_invalid_request() {
        let envelope = RuntimeErrorEnvelope::from(&pinned_mismatch());
        assert_eq!(envelope.r#type, "invalid_request_error");
        assert!(!envelope.retryable);
    }

    /// The wire-vocabulary predicate must agree with `is_retryable()` for every
    /// variant.
    ///
    /// An HTTP client only ever sees the `type` discriminant — it has no
    /// `ModelRuntimeError` to ask. This is what stops the two from drifting
    /// into disagreeing about which failures are worth retrying, and it is
    /// exhaustive so a new variant cannot quietly skip the check.
    #[test]
    fn retryable_predicate_agrees_with_the_error_itself() {
        let all = [
            ModelRuntimeError::ModelLoading,
            ModelRuntimeError::AdmissionTimeout("contended".to_string()),
            ModelRuntimeError::ModelNotFound("m".to_string()),
            ModelRuntimeError::ModelFileNotFound("f".to_string()),
            pinned_mismatch(),
            ModelRuntimeError::SpawnFailed("boom".to_string()),
            ModelRuntimeError::HealthCheckFailed("unhealthy".to_string()),
            ModelRuntimeError::Internal("internal".to_string()),
        ];

        for err in all {
            let envelope = RuntimeErrorEnvelope::from(&err);
            assert_eq!(
                is_retryable_error_type(&envelope.r#type),
                err.is_retryable(),
                "wire type {:?} disagrees with is_retryable() for {err:?}",
                envelope.r#type
            );
        }
    }

    /// A 503 is the only status the retryable discriminant maps to, so the
    /// HTTP-status fallback used by clients that receive a non-gglib error
    /// body stays consistent with the discriminant path.
    #[test]
    fn retryable_discriminant_lines_up_with_status_503() {
        let retryable = ModelRuntimeError::AdmissionTimeout("c".to_string());
        assert!(is_retryable_error_type(error_type::SERVICE_UNAVAILABLE));
        assert_eq!(retryable.suggested_status_code(), 503);
        assert!(!is_retryable_error_type(error_type::SERVER_ERROR));
        assert!(!is_retryable_error_type(error_type::INVALID_REQUEST));
    }
}
