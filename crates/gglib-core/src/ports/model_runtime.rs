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
use crate::domain::{
    AdmissionSnapshot, CacheRamHealth, ComponentRole, LaunchNarration, ModelSamplingDefaults,
    RuntimeKind,
};
use crate::ports::ProcessHandle;
pub use crate::ports::pinned::PinnedSpec;
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

/// Target information for a running model instance: all a caller needs to
/// route requests to a running llama-server.
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
    /// The program serving the model: llama.cpp's `llama-server` unless the
    /// launch says otherwise. A reader that speaks llama-server's own API
    /// (`/slots`, `/props`) skips a target served by `sd-server`.
    pub runtime: RuntimeKind,
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
            runtime: RuntimeKind::Llama,
        }
    }

    /// Say which program serves the model.
    #[must_use]
    pub const fn with_runtime(mut self, runtime: RuntimeKind) -> Self {
        self.runtime = runtime;
        self
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

    /// The request holding `slot` has made progress without finishing (an
    /// image render's step), which proves the queue is moving: waiters behind
    /// it start their stall clocks again.
    ///
    /// Required, so no implementation can forget that a long render is not a
    /// wedge. The same rules as [`Self::release`]: never block, panic, or
    /// await.
    fn progress(&self, slot: usize);
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

    /// Report that the request holding this lease has made progress (see
    /// [`AdmissionRelease::progress`]).
    pub fn progress(&self) {
        if let Some(owner) = &self.owner {
            owner.progress(self.slot);
        }
    }

    /// Drop the lease without releasing it.
    ///
    /// Only for a teardown that has already settled this lease's count under
    /// the queue's lock (an image render retired after its process was
    /// killed). Releasing it as well would take one in-flight request away
    /// from whatever model holds the slot by then.
    pub fn disarm(mut self) {
        self.owner = None;
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

    /// A model is currently loading; try again later (a 503).
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
        /// The name of the model the request resolved to.
        requested: String,
    },

    /// The model draws images, and llama-server, which serves chat, cannot
    /// load it.
    #[error("Model '{0}' is an image model: it draws images and cannot be served for chat")]
    ImageModelCannotChat(String),

    /// An image model was asked for and stable-diffusion.cpp's `sd-server`,
    /// which draws, is not installed. Not retryable: installing it is the
    /// fix, and the message names the command.
    #[error(
        "The image runtime, stable-diffusion.cpp's sd-server, is not installed. Install it \
         with `{cmd}`, or from Settings → Image runtime.",
        cmd = crate::paths::SD_INSTALL_COMMAND
    )]
    ImageRuntimeNotInstalled,

    /// An image model's family needs files it has none linked for, so
    /// `sd-server` could not load it.
    #[error("{}", image_refusal::incomplete(model, missing))]
    ImageModelIncomplete {
        /// The model's name.
        model: String,
        /// Every role its family needs and it has no file for, in the
        /// recipe's order.
        missing: Vec<ComponentRole>,
    },

    /// Retryable: an image model needs more memory than is free, and the
    /// model it would displace is held by a run, so it cannot be swapped out
    /// now. Refused at once rather than queued: the hold can last as long as
    /// the run does.
    #[error(
        "{}",
        image_refusal::does_not_fit(model, held_model, *needed_bytes, *free_bytes)
    )]
    ImageModelDoesNotFit {
        /// The image model's name.
        model: String,
        /// The resident model that is held and would have to go.
        held_model: String,
        /// What the image model needs, its files and its family's margin,
        /// when the verdict knew it.
        needed_bytes: Option<u64>,
        /// What is free beside the held model, when it can be read.
        free_bytes: Option<u64>,
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
        matches!(
            self,
            Self::ModelLoading | Self::AdmissionTimeout(_) | Self::ImageModelDoesNotFit { .. }
        )
    }

    /// Returns a suggested HTTP status code for this error.
    #[must_use]
    pub const fn suggested_status_code(&self) -> u16 {
        match self {
            Self::ModelLoading
            | Self::AdmissionTimeout(_)
            | Self::ImageModelDoesNotFit { .. }
            | Self::ImageRuntimeNotInstalled => 503,
            // A pinned mismatch is 404, not 403: from the client's point of
            // view the model it asked for does not exist on this endpoint.
            Self::ModelNotFound(_)
            | Self::ModelFileNotFound(_)
            | Self::PinnedModelMismatch { .. } => 404,
            // The model exists; the request named the wrong kind of model,
            // or one that is missing a file it needs.
            Self::ImageModelCannotChat(_) | Self::ImageModelIncomplete { .. } => 400,
            Self::SpawnFailed(_) | Self::HealthCheckFailed(_) | Self::Internal(_) => 500,
        }
    }
}

/// Canonical `error.type` discriminants, shared by every surface.
///
/// `gglib_proxy::models::ErrorResponse` carries one of these over HTTP and
/// [`RuntimeErrorEnvelope`] carries the same vocabulary over SSE, so a client
/// that learns it once understands both.
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
/// For event boundaries (SSE) that need machine-readable type +
/// retry hints alongside the human-readable message, mirroring the shape
/// `gglib_proxy::models::ErrorResponse` already sends over HTTP.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
            ModelRuntimeError::ModelLoading
            | ModelRuntimeError::AdmissionTimeout(_)
            | ModelRuntimeError::ImageModelDoesNotFit { .. } => error_type::SERVICE_UNAVAILABLE,
            ModelRuntimeError::ModelNotFound(_)
            | ModelRuntimeError::ModelFileNotFound(_)
            | ModelRuntimeError::PinnedModelMismatch { .. }
            | ModelRuntimeError::ImageModelCannotChat(_)
            | ModelRuntimeError::ImageModelIncomplete { .. } => error_type::INVALID_REQUEST,
            // A 503 that retrying cannot fix: nothing changes until someone
            // installs the runtime.
            ModelRuntimeError::SpawnFailed(_)
            | ModelRuntimeError::HealthCheckFailed(_)
            | ModelRuntimeError::ImageRuntimeNotInstalled
            | ModelRuntimeError::Internal(_) => error_type::SERVER_ERROR,
        };
        Self {
            message: err.to_string(),
            r#type: discriminant.to_string(),
            retryable: err.is_retryable(),
        }
    }
}

/// Told where a request waiting in the admission queue stands.
///
/// Called with no lock held, only after the queue has answered "wait" and
/// only when the place has changed since the last call. Must not block: a
/// slow observer delays the admission it describes.
pub trait AdmitObserver: Send + Sync + fmt::Debug {
    /// The request is waiting, `position` in line, 1 being next.
    fn queued(&self, position: usize);
}

/// Port for admitting requests to a running model: the proxy's way to a
/// running model server. Implementations handle:
/// - Model resolution (name → file path)
/// - Process lifecycle (start, stop, health check)
/// - Context size management
/// - Admission control: queueing, batching, and the VRAM resident set
#[async_trait]
pub trait ModelRuntimePort: Send + Sync + fmt::Debug {
    /// Admit a request to a running model, launching or swapping if needed.
    ///
    /// This method:
    /// 1. Resolves the model id or name to a database entry, and refuses it
    ///    if the runtime is pinned to another (see [`Self::pinned`])
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
    /// * `model_name` - Id or exact name of the model to run
    /// * `num_ctx` - Optional context size override from request
    /// * `default_ctx` - Default context size if not specified
    /// * `overrides` - Per-call launch options layered on the runtime's
    ///   standing template, so one shared runtime can serve callers with
    ///   different launch needs (a GUI start carrying `--load-mode mmap+mlock`, a benchmark
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
        default_ctx: Option<u64>,
        overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError>;

    /// [`Self::admit`], telling `observer` the request's place in line each
    /// time it is asked to wait and that place has changed, so a person
    /// waiting on a long admission (an image render's model behind a chat)
    /// sees why nothing is happening yet.
    ///
    /// Defaults to [`Self::admit`], telling the observer nothing: a runtime
    /// with no queue never waits.
    ///
    /// # Errors
    ///
    /// As [`Self::admit`].
    async fn admit_observed(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: Option<u64>,
        overrides: LaunchOverrides,
        observer: Option<Arc<dyn AdmitObserver>>,
    ) -> Result<Admission, ModelRuntimeError> {
        let _ = observer;
        self.admit(model_name, num_ctx, default_ctx, overrides)
            .await
    }

    /// What the admission queue and the VRAM resident set look like right now.
    ///
    /// Synchronous for the same reason [`Self::pinned`] is: it is a
    /// single read of plain shared state, not a query against live process
    /// state. The dashboard publisher calls it on every tick.
    ///
    /// Defaults to empty for runtimes with no resident set to report (test
    /// doubles, remote backends).
    fn admission_snapshot(&self) -> AdmissionSnapshot {
        AdmissionSnapshot::default()
    }

    /// The currently running model, or `None` if there is none.
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

    /// Keep model `model_id`, listening on `port`, resident — neither swapped
    /// out nor recycled — until the returned lease drops.
    ///
    /// For a caller that talks to llama-server's port directly rather than
    /// through [`Self::admit`] — an agent run — so a proxy request cannot take
    /// the model from under it. Unlike an admission it takes none of the
    /// model's parallel capacity. `None` when that model is not the one on
    /// `port`, and by default, for runtimes with no resident set.
    fn hold(&self, port: u16, model_id: u32) -> Option<AdmissionLease> {
        let _ = (port, model_id);
        None
    }

    /// Stop the current model, even one a run holds: an explicit stop (a
    /// person's, a benchmark's), or the proxy's restart of a dead server.
    async fn stop_current(&self) -> Result<(), ModelRuntimeError>;

    /// Stop model `model_id` wherever it is resident, the primary slot or the
    /// second, even one a run holds: a person's Stop on a model they chose,
    /// which is often an image model beside a chat model.
    ///
    /// Required of every runtime, unlike [`Self::list_running`]'s default:
    /// a runtime that listed a model and could not stop it would show a Stop
    /// that does nothing. `Ok(false)` when that model is not running here,
    /// which a caller reports as "not found"; `Ok(true)` once it is stopped.
    async fn stop_model(&self, model_id: u32) -> Result<bool, ModelRuntimeError>;

    /// Stop it for automatic recovery, which waits for a run: refused with
    /// [`ModelRuntimeError::AdmissionTimeout`] while one holds it (see
    /// [`Self::hold`]). Defaults to [`Self::stop_current`], for runtimes
    /// with no holds.
    async fn recycle_current(&self) -> Result<(), ModelRuntimeError> {
        self.stop_current().await
    }

    /// The one model this runtime is pinned to, if any.
    ///
    /// `Some(pin)` means a request for any model whose id is not `pin.id` is
    /// refused with [`ModelRuntimeError::PinnedModelMismatch`] rather than
    /// swapped to — the mode `gglib serve` runs in. `None` is the ordinary
    /// auto-swapping runtime.
    ///
    /// Synchronous because the pin is plain shared state, unlike
    /// [`Self::current_model`], which reports live process state. Owned
    /// rather than borrowed because the pin can change at runtime (see
    /// [`Self::set_pin`]) — a borrow could not outlive the lock guarding it.
    ///
    /// Defaults to unpinned so test doubles and remote backends need not
    /// implement it. Callers use it to avoid offering a model that would only
    /// be refused — `/v1/models` being the motivating case.
    fn pinned(&self) -> Option<PinnedSpec> {
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

/// A [`ModelRuntimePort`] that never has anything running.
///
/// For code that has to hold a runtime and asks it nothing that matters: a
/// test of something that starts no model. Not for a process that runs
/// beside the one serving models. There "nothing is running" would be a
/// claim about that other process, so the CLI's one-shot commands read the
/// pid files under their data root instead (`RecordedServers` in
/// `gglib-cli`).
#[derive(Debug, Default)]
pub struct NoopModelRuntime;

#[async_trait]
impl ModelRuntimePort for NoopModelRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
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

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

#[path = "model_runtime_image_refusal.rs"]
mod image_refusal;

#[cfg(test)]
#[path = "model_runtime_tests.rs"]
mod tests;
