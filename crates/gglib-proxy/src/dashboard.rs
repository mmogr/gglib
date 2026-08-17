//! Unified proxy dashboard data contract.
//!
//! [`DashboardSnapshot`] aggregates the state tracked separately by
//! [`crate::connections::ActiveConnectionsRegistry`],
//! [`crate::slots_poller::SlotsCache`], and
//! [`crate::metrics::ContextMetricsStore`] into the single JSON shape
//! returned by `GET /v1/proxy/status` and pushed (via
//! [`gglib_sse::Broadcaster`]) over `GET /v1/proxy/status/stream`.
//!
//! This fully replaces the old `{snapshots, total_requests}` response shape
//! — there is no back-compat shim. Nothing outside this crate consumed the
//! old shape (it was explicitly documented as a not-yet-consumed "future"
//! data contract), so the replacement is a clean cut, not an additive
//! extension.
//!
//! ## Live updates without spreading broadcast plumbing everywhere
//!
//! An alternative design would thread a broadcast call into every mutation
//! site across `forward.rs` and `connections.rs`
//! (firing a `DashboardEvent` on every progress tick, connection start/end,
//! and slots poll). That would work, but it spreads dashboard-specific
//! concerns into modules that otherwise have nothing to do with it.
//!
//! Instead, [`spawn_dashboard_publisher`] runs a small dedicated task that
//! recomputes the full aggregate [`DashboardSnapshot`] on a short interval
//! and pushes it to subscribers. Every underlying store stays completely
//! ignorant of the dashboard/broadcast concern, and subscribers still get
//! near-real-time updates — the same cadence as the `/slots` poller itself,
//! so slot data is never staler than what's already being polled.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::domain::CacheRamHealth;
use gglib_sse::Broadcaster;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use gglib_core::domain::AdmissionSnapshot;
use gglib_core::ports::ModelRuntimePort;

use crate::connections::{ActiveConnectionSnapshot, ActiveConnectionsRegistry};
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::sampling_audit::{SamplingAuditSnapshot, SamplingAuditStore};
use crate::slots::{SlotSnapshot, SlotsPollResult};
use crate::slots_poller::SlotsCache;
use crate::upstream_health::{UpstreamHealth, UpstreamHealthSnapshot};
use gglib_core::cache_metrics::{CacheMetricsStore, CacheUsage};
use gglib_core::domain::LaunchNarration;

/// Number of recent request snapshots included in each [`DashboardSnapshot`].
const RECENT_REQUEST_LIMIT: usize = 20;

// =============================================================================
// CacheStatus
// =============================================================================

/// How prompt caching is configured for the currently running model.
///
/// Grouped into its own object rather than flattened onto
/// [`DashboardSnapshot`] so cache reporting has one place to grow: this is the
/// extension point for per-request cache telemetry (tokens reused, TTFT
/// saved), which would otherwise accumulate as unrelated top-level fields.
///
/// The fields directly on this struct are *configuration* — resolved once when
/// a model is launched and changing only on a model swap. Per-request
/// measurements live under [`Self::usage`] rather than being mixed in, so a
/// consumer can tell "how the cache is set up" from "what it actually did".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CacheStatus {
    /// Whether disk KV slot persistence is enabled on this proxy instance
    /// (`--cache` + `--slot-dir`).
    pub disk_enabled: bool,
    /// Whether the disk layer is enabled but suppressed for the running model
    /// because its attention keeps only part of the token history. Always
    /// `false` when [`Self::disk_enabled`] is `false` — there is nothing to
    /// suppress.
    pub disk_suppressed_for_model: bool,
    /// Resolved `--cache-ram` budget in MiB. `None` when no flag was emitted
    /// and llama-server's own default applies.
    pub ram_budget_mb: Option<u64>,
    /// Stable machine-readable label for the budget's health, for styling.
    /// One of `healthy`, `low`, `disabled_insufficient_ram`,
    /// `disabled_by_user`, `llama_default`.
    pub ram_state: &'static str,
    /// Whether any of the below warrants surfacing to the user. `false` when
    /// everything is either healthy or deliberately configured.
    pub needs_attention: bool,
    /// Ready-to-render warning lines, empty when nothing is wrong. Phrased for
    /// display rather than parsing — consumers should branch on
    /// [`Self::ram_state`] and [`Self::disk_suppressed_for_model`].
    pub warnings: Vec<String>,
    /// Measured prompt-cache reuse since the proxy started. Unlike the fields
    /// above, this changes on every request.
    pub usage: CacheUsage,
}

impl CacheStatus {
    /// Build the status for a given disk-layer configuration and resolved RAM
    /// budget health.
    ///
    /// `slot_restore_supported` mirrors
    /// [`gglib_core::ports::RunningTarget::slot_restore_supported`]; it is only
    /// meaningful when `disk_enabled` is true.
    #[must_use]
    pub fn build(
        disk_enabled: bool,
        slot_restore_supported: bool,
        ram_health: CacheRamHealth,
    ) -> Self {
        let disk_suppressed_for_model = disk_enabled && !slot_restore_supported;

        let ram_state = match ram_health {
            CacheRamHealth::Healthy { .. } => "healthy",
            CacheRamHealth::Low { .. } => "low",
            CacheRamHealth::DisabledInsufficientRam => "disabled_insufficient_ram",
            CacheRamHealth::DisabledByUser => "disabled_by_user",
            CacheRamHealth::LlamaDefault => "llama_default",
        };

        let ram_budget_mb = match ram_health {
            CacheRamHealth::Healthy { mb } | CacheRamHealth::Low { mb } => Some(mb),
            CacheRamHealth::DisabledByUser | CacheRamHealth::DisabledInsufficientRam => Some(0),
            CacheRamHealth::LlamaDefault => None,
        };

        let mut warnings = Vec::new();
        match ram_health {
            CacheRamHealth::Low { mb } => warnings.push(format!(
                "Low memory available for prompt caching ({mb} MiB) — switching between \
                 conversations will often re-process the prompt from scratch."
            )),
            CacheRamHealth::DisabledInsufficientRam => warnings.push(
                "Prompt caching is off: this model's weights and KV cache leave no room for \
                 it. Reduce the context size or use a smaller model to enable it."
                    .to_string(),
            ),
            CacheRamHealth::Healthy { .. }
            | CacheRamHealth::DisabledByUser
            | CacheRamHealth::LlamaDefault => {}
        }

        // Deliberately additive: on a low-RAM machine running a hybrid model
        // both tiers are degraded at once, and that combination is exactly
        // when the user most needs to understand why things are slow.
        if disk_suppressed_for_model {
            warnings.push(
                "Disk cache offloading is disabled for this model — its attention keeps only \
                 part of the token history, which llama-server's slot files can't restore."
                    .to_string(),
            );
        }

        Self {
            disk_enabled,
            disk_suppressed_for_model,
            ram_budget_mb,
            ram_state,
            needs_attention: ram_health.needs_attention() || disk_suppressed_for_model,
            warnings,
            // Config-only at construction; the live figure is attached at
            // snapshot time via `with_usage`. See `CacheStatusCache`.
            usage: CacheUsage::default(),
        }
    }

    /// Attach measured reuse totals to an otherwise config-only status.
    #[must_use]
    pub fn with_usage(mut self, usage: CacheUsage) -> Self {
        self.usage = usage;
        self
    }
}

/// Latest observed cache configuration, written by the request path as models
/// resolve and read by the dashboard publisher.
///
/// Mirrors [`crate::slots_poller::SlotsCache`]: a small mutex-guarded cell
/// shared between a writer that learns the value incidentally and a reader
/// that needs it on its own schedule. `None` until the first request resolves
/// a model, since the RAM budget isn't known until something is launched.
///
/// Holds the **configuration** half only — every stored value carries a
/// default [`CacheUsage`]. Reuse totals move on every request and would defeat
/// the unchanged-write skip below, so they are read live from
/// [`gglib_core::cache_metrics::CacheMetricsStore`] and attached in
/// [`DashboardSnapshot::build`] via [`CacheStatus::with_usage`].
#[derive(Debug, Default)]
pub struct CacheStatusCache {
    latest: std::sync::Mutex<Option<CacheStatus>>,
}

impl CacheStatusCache {
    /// Create an empty cache ("no model resolved yet").
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The most recently observed cache configuration.
    #[must_use]
    pub fn get(&self) -> Option<CacheStatus> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Record the configuration of a freshly resolved target.
    ///
    /// Skips the write when nothing changed, so the steady state (every
    /// request resolving the same model) never contends with the publisher.
    pub fn set(&self, status: CacheStatus) {
        let mut guard = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref() != Some(&status) {
            *guard = Some(status);
        }
    }
}

/// Latest launch narration, written by the request path as models resolve
/// and read by the dashboard publisher.
///
/// Mirrors [`CacheStatusCache`] exactly, and for the same reason: the launch
/// decisions and their provenance exist only at spawn, inside the runtime,
/// while the dashboard that displays them lives here. The resolved target is
/// where the two meet.
///
/// `None` until the first request resolves a model — before that, there is no
/// launch to narrate.
#[derive(Debug, Default)]
pub struct LaunchNarrationCache {
    latest: std::sync::Mutex<Option<LaunchNarration>>,
}

impl LaunchNarrationCache {
    /// Create an empty cache ("nothing launched yet").
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The most recently observed launch narration.
    #[must_use]
    pub fn get(&self) -> Option<LaunchNarration> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Record the narration of a freshly resolved target.
    ///
    /// Skips the write when nothing changed, so the steady state — every
    /// request resolving the same model — never contends with the publisher.
    pub fn set(&self, narration: LaunchNarration) {
        let mut guard = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref() != Some(&narration) {
            *guard = Some(narration);
        }
    }
}

/// Cadence at which a fresh snapshot is recomputed and pushed to SSE
/// subscribers of `GET /v1/proxy/status/stream`.
const PUBLISH_INTERVAL: Duration = Duration::from_secs(1);

/// Broadcast channel capacity. A slow subscriber can fall behind by this
/// many snapshots before missing one — harmless here, since every snapshot
/// is a full state dump and the next tick supersedes whatever was missed.
const BROADCAST_CAPACITY: usize = 8;

// =============================================================================
// DashboardSnapshot
// =============================================================================

/// The single, unified proxy dashboard data contract.
///
/// This is both the `GET /v1/proxy/status` response body and the event type
/// pushed over `GET /v1/proxy/status/stream`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DashboardSnapshot {
    /// Every currently in-flight model request — direct `/v1/chat/completions`
    /// completions and `/v1/embeddings`.
    pub active_connections: Vec<ActiveConnectionSnapshot>,
    /// `true` if the running llama-server's `/slots` endpoint is reachable
    /// and enabled. `false` if it's disabled (`--no-slots`) or currently
    /// unreachable — see [`Self::slots_status`] for the reason in that case.
    pub slots_available: bool,
    /// Per-slot context usage, populated when [`Self::slots_available`] is
    /// `true`. Empty otherwise.
    pub slots: Vec<SlotSnapshot>,
    /// Human-readable reason `slots` is empty. `None` when slots are
    /// available; otherwise either the disabled notice or the poller's
    /// last connect/timeout/parse error message.
    pub slots_status: Option<String>,
    /// Most recent request snapshots, oldest first, capped at
    /// [`RECENT_REQUEST_LIMIT`].
    pub recent_requests: Vec<ContextSnapshot>,
    /// Total requests handled since the proxy started, including any
    /// evicted from `recent_requests`'s ring buffer.
    pub total_requests: u64,
    /// Total requests whose client-visible output carried dialect residue
    /// (the drift alarm), eviction-safe like `total_requests`.
    pub dialect_residue_total: u64,
    /// Turns whose tool call failed schema validation and was re-issued with
    /// `tool_choice: "required"`, counted whether or not the re-issue worked.
    ///
    /// A sustained non-zero rate says this model's `auto` path is
    /// unconstrained upstream — the per-model signal ADR 0002 could otherwise
    /// only read from a `--verbose` llama-server log.
    pub tool_repairs_attempted: u64,
    /// Of those, the ones that produced a conformant call. The ratio is the
    /// number worth watching.
    pub tool_repairs_succeeded: u64,
    /// Upstream-degradation watchdog counters (empty responses, first-byte
    /// timeouts, proactive recycles) since the proxy started.
    pub upstream_health: UpstreamHealthSnapshot,
    /// Per-model defect counts, keyed by the model name requests carry.
    ///
    /// The fleet totals above answer "is something wrong"; this answers
    /// "with which model", which is the only form the answer is actionable
    /// in — a rate is a claim about a model, not about traffic.
    ///
    /// Process-lifetime and reset on restart, deliberately (ADR 0006): a
    /// defect rate is a claim about *recent* traffic on *this* build.
    pub per_model_defects:
        std::collections::HashMap<String, gglib_core::domain::defects::ModelDefectCounts>,
    /// How prompt caching is configured for the running model. `None` until
    /// the first request resolves a model, since the RAM budget isn't known
    /// until something is launched.
    pub cache: Option<CacheStatus>,
    /// Prompt-cache reuse for the in-process **agent path** — GUI
    /// chat, which talk to llama-server directly rather than through
    /// [`crate::forward`]. Reported as a separate population from [`Self::cache`]'s
    /// `usage`, never merged into it: an agent turn's many small tool-driven calls
    /// have a reuse profile nothing like a user's conversation. Top-level rather
    /// than nested under [`Self::cache`] because it does not depend on the
    /// proxy's cache configuration and must surface even before a proxied
    /// request has resolved a model.
    pub agent_usage: CacheUsage,
    /// What the running model's launch decided, and why — the same record the
    /// CLI banner prints at startup (see
    /// [`gglib_core::domain::LaunchNarration`]). `None` until a request has
    /// resolved a model, since nothing has been launched to explain.
    pub launch: Option<LaunchNarration>,
    /// Which models hold VRAM slots, what is queued behind them, and why the
    /// second slot is or is not in use.
    ///
    /// Read live from the runtime on every snapshot rather than cached the way
    /// [`Self::cache`] and [`Self::launch`] are. Those two change only when a
    /// model is launched, so a write-on-resolution cache reports them
    /// faithfully; queue depth changes continuously and without any request
    /// resolving a model, so a cached copy would be wrong precisely when it
    /// mattered — while traffic was backing up.
    pub admission: AdmissionSnapshot,
    /// Whether the sampling gglib resolved is the sampling llama-server
    /// applied — and, first, whether anyone is in a position to know.
    ///
    /// The Tier C organ ADR 0001 says makes the other tiers honest, and which
    /// sampling did not have. Consumers must render
    /// [`AuditState::Blind`](crate::sampling_audit::AuditState::Blind)
    /// differently from `Comparing { divergences: 0 }`: a silent instrument
    /// and a healthy one produce the same number and mean opposite things.
    ///
    /// It also carries the two records that are *not* comparisons, because
    /// nothing on the wire echoes their subject: the reasoning controls
    /// ([`ReasoningReadback`](crate::audit_records::ReasoningReadback), with the
    /// running template's tri-state answer on `reasoning_effort`) and the client
    /// field names the trust gate dropped. They ride here rather than beside
    /// this field because the same store owns both the `/props` reading and the
    /// resolved record — a peer field would need a second handle to it, and two
    /// handles to one store are two things to keep in step.
    pub sampling_audit: SamplingAuditSnapshot,
}

impl DashboardSnapshot {
    /// Build a fresh snapshot by reading the three underlying state
    /// sources. Cheap: each source's read is a single mutex-guarded clone,
    /// none held across an `.await`.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        connections: &ActiveConnectionsRegistry,
        slots: &SlotsCache,
        metrics: &ContextMetricsStore,
        upstream_health: &UpstreamHealth,
        cache: &CacheStatusCache,
        cache_metrics: &CacheMetricsStore,
        agent_metrics: &CacheMetricsStore,
        launch: &LaunchNarrationCache,
        runtime: Option<&dyn ModelRuntimePort>,
        sampling_audit: &SamplingAuditStore,
    ) -> Self {
        let (slots_available, slots_vec, slots_status) = match slots.get() {
            SlotsPollResult::Available(snapshots) => (true, snapshots, None),
            SlotsPollResult::Disabled => (
                false,
                Vec::new(),
                Some("disabled upstream (--no-slots)".to_string()),
            ),
            SlotsPollResult::Unreachable(reason) => (false, Vec::new(), Some(reason)),
        };

        Self {
            active_connections: connections.snapshot(),
            slots_available,
            slots: slots_vec,
            slots_status,
            recent_requests: metrics.recent(RECENT_REQUEST_LIMIT),
            total_requests: metrics.total_requests(),
            dialect_residue_total: metrics.dialect_residue_total(),
            tool_repairs_attempted: metrics.tool_repairs_attempted(),
            tool_repairs_succeeded: metrics.tool_repairs_succeeded(),
            upstream_health: upstream_health.snapshot(),
            per_model_defects: metrics.defect_counts(),
            // Stored config plus live reuse totals — see `CacheStatusCache`.
            cache: cache
                .get()
                .map(|status| status.with_usage(cache_metrics.snapshot())),
            agent_usage: agent_metrics.snapshot(),
            launch: launch.get(),
            // `None` only in tests that build a snapshot without a runtime to
            // ask; an empty resident set is the honest answer there.
            admission: runtime
                .map(ModelRuntimePort::admission_snapshot)
                .unwrap_or_default(),
            sampling_audit: sampling_audit.snapshot(),
        }
    }
}

// =============================================================================
// DashboardState
// =============================================================================

/// Shared handle to the dashboard's underlying stores plus the SSE
/// broadcaster that pushes [`DashboardSnapshot`]s to subscribers.
///
/// Consolidates what used to be three separate `AppState` fields
/// (`metrics`, `connections`, `slots`) into one, per the "no backwards
/// compatibility" design for this phase.
pub struct DashboardState {
    pub connections: Arc<ActiveConnectionsRegistry>,
    pub slots: Arc<SlotsCache>,
    pub metrics: Arc<ContextMetricsStore>,
    pub upstream_health: Arc<UpstreamHealth>,
    pub broadcaster: Arc<Broadcaster<DashboardSnapshot>>,
    /// Latest observed prompt-cache configuration, populated by the request
    /// path as models resolve.
    pub cache: Arc<CacheStatusCache>,
    /// Running prompt-cache reuse totals, recorded by the forward paths.
    pub cache_metrics: Arc<CacheMetricsStore>,
    /// Agent-path prompt-cache reuse totals — a separate population from
    /// [`Self::cache_metrics`], recorded by GUI-chat runs (which
    /// bypass [`crate::forward`]) via [`gglib_core::ports::UsageSink`].
    /// Owned by the supervisor and passed in, so it outlives a single proxy
    /// run and can be shared with the embedded axum server.
    pub agent_metrics: Arc<CacheMetricsStore>,
    /// Latest launch narration, populated by the request path as models
    /// resolve. Created here rather than passed in: unlike
    /// [`Self::cache_metrics`], nothing outside the proxy writes to it.
    pub launch: Arc<LaunchNarrationCache>,
    /// The runtime, asked for its admission state on every snapshot.
    ///
    /// Held here rather than reached through `AppState` because the publisher
    /// task runs on its own schedule with no request in hand to borrow from.
    pub runtime: Arc<dyn ModelRuntimePort>,
    /// Tier C sampling readback: what gglib resolved, what llama-server
    /// reported, and whether the organ can see at all. Written by both the
    /// request path and the `/slots` poller — see [`crate::sampling_audit`].
    pub sampling_audit: Arc<SamplingAuditStore>,
}

impl DashboardState {
    /// Construct a fresh `DashboardState` wrapping the given stores, with a
    /// new (empty, zero-subscriber) broadcaster.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connections: Arc<ActiveConnectionsRegistry>,
        slots: Arc<SlotsCache>,
        metrics: Arc<ContextMetricsStore>,
        upstream_health: Arc<UpstreamHealth>,
        cache: Arc<CacheStatusCache>,
        cache_metrics: Arc<CacheMetricsStore>,
        agent_metrics: Arc<CacheMetricsStore>,
        runtime: Arc<dyn ModelRuntimePort>,
        sampling_audit: Arc<SamplingAuditStore>,
    ) -> Self {
        Self {
            connections,
            slots,
            metrics,
            upstream_health,
            broadcaster: Arc::new(Broadcaster::new(BROADCAST_CAPACITY)),
            cache,
            cache_metrics,
            agent_metrics,
            launch: Arc::new(LaunchNarrationCache::new()),
            runtime,
            sampling_audit,
        }
    }

    /// Compute the current [`DashboardSnapshot`] from the underlying
    /// stores.
    #[must_use]
    pub fn snapshot(&self) -> DashboardSnapshot {
        DashboardSnapshot::build(
            &self.connections,
            &self.slots,
            &self.metrics,
            &self.upstream_health,
            &self.cache,
            &self.cache_metrics,
            &self.agent_metrics,
            &self.launch,
            Some(self.runtime.as_ref()),
            &self.sampling_audit,
        )
    }
}

// =============================================================================
// Publisher task
// =============================================================================

/// Spawn the background task that recomputes and broadcasts a fresh
/// [`DashboardSnapshot`] every [`PUBLISH_INTERVAL`].
///
/// Mirrors [`crate::slots_poller::spawn_slots_poller`]'s cancellation-aware
/// sleep via `tokio::select!`, so it shuts down promptly — never sleeping
/// out a full interval — when `cancel` fires. `serve()` awaits the returned
/// `JoinHandle` after `axum::serve` completes, so this task is always
/// joined, never left detached.
pub fn spawn_dashboard_publisher(
    state: Arc<DashboardState>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = cancel.cancelled() => {
                    debug!("proxy dashboard: publisher shutting down");
                    return;
                }
                () = tokio::time::sleep(PUBLISH_INTERVAL) => {}
            }
            state.broadcaster.send(state.snapshot());
        }
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "dashboard_tests.rs"]
mod dashboard_tests;
