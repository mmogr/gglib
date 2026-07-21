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
//! site across `forward.rs`, `council_proxy.rs`, and `connections.rs`
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

use crate::connections::{ActiveConnectionSnapshot, ActiveConnectionsRegistry};
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::slots::{SlotSnapshot, SlotsPollResult};
use crate::slots_poller::SlotsCache;
use crate::upstream_health::{UpstreamHealth, UpstreamHealthSnapshot};
use gglib_core::cache_metrics::{CacheMetricsStore, CacheUsage};

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
    /// Every currently in-flight `/v1/chat/completions` request — both
    /// direct completions and council/virtual-model runs.
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
    /// Upstream-degradation watchdog counters (empty responses, first-byte
    /// timeouts, proactive recycles) since the proxy started.
    pub upstream_health: UpstreamHealthSnapshot,
    /// How prompt caching is configured for the running model. `None` until
    /// the first request resolves a model, since the RAM budget isn't known
    /// until something is launched.
    pub cache: Option<CacheStatus>,
}

impl DashboardSnapshot {
    /// Build a fresh snapshot by reading the three underlying state
    /// sources. Cheap: each source's read is a single mutex-guarded clone,
    /// none held across an `.await`.
    #[must_use]
    pub fn build(
        connections: &ActiveConnectionsRegistry,
        slots: &SlotsCache,
        metrics: &ContextMetricsStore,
        upstream_health: &UpstreamHealth,
        cache: &CacheStatusCache,
        cache_metrics: &CacheMetricsStore,
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
            upstream_health: upstream_health.snapshot(),
            // Stored config plus live reuse totals — see `CacheStatusCache`.
            cache: cache
                .get()
                .map(|status| status.with_usage(cache_metrics.snapshot())),
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
}

impl DashboardState {
    /// Construct a fresh `DashboardState` wrapping the given stores, with a
    /// new (empty, zero-subscriber) broadcaster.
    #[must_use]
    pub fn new(
        connections: Arc<ActiveConnectionsRegistry>,
        slots: Arc<SlotsCache>,
        metrics: Arc<ContextMetricsStore>,
        upstream_health: Arc<UpstreamHealth>,
        cache: Arc<CacheStatusCache>,
        cache_metrics: Arc<CacheMetricsStore>,
    ) -> Self {
        Self {
            connections,
            slots,
            metrics,
            upstream_health,
            broadcaster: Arc::new(Broadcaster::new(BROADCAST_CAPACITY)),
            cache,
            cache_metrics,
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
mod tests {
    use super::*;
    use futures_util::StreamExt;

    fn empty_state() -> Arc<DashboardState> {
        Arc::new(DashboardState::new(
            Arc::new(ActiveConnectionsRegistry::new()),
            Arc::new(SlotsCache::new()),
            Arc::new(ContextMetricsStore::new()),
            Arc::new(UpstreamHealth::new()),
            Arc::new(CacheStatusCache::new()),
            Arc::new(CacheMetricsStore::new()),
        ))
    }

    #[test]
    fn build_aggregates_empty_stores() {
        let connections = ActiveConnectionsRegistry::new();
        let slots = SlotsCache::new();
        let metrics = ContextMetricsStore::new();
        let upstream_health = UpstreamHealth::new();

        let snapshot = DashboardSnapshot::build(
            &connections,
            &slots,
            &metrics,
            &upstream_health,
            &CacheStatusCache::new(),
            &CacheMetricsStore::new(),
        );

        assert!(snapshot.active_connections.is_empty());
        assert!(!snapshot.slots_available);
        assert!(snapshot.slots.is_empty());
        assert!(snapshot.slots_status.is_some());
        assert!(snapshot.recent_requests.is_empty());
        assert_eq!(snapshot.total_requests, 0);
    }

    #[test]
    fn build_reflects_active_connection_and_metrics() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let _guard = connections.register("qwen-3b", true, Some(4096));
        let slots = SlotsCache::new();
        let metrics = ContextMetricsStore::new();
        metrics.record(crate::metrics::ContextSnapshot {
            model_name: "qwen-3b".to_string(),
            payload_chars_before: 100,
            payload_chars_after: 100,
            messages_truncated: 0,
            was_clamped: false,
            recorded_at_secs: 0,
        });

        let upstream_health = UpstreamHealth::new();
        let snapshot = DashboardSnapshot::build(
            &connections,
            &slots,
            &metrics,
            &upstream_health,
            &CacheStatusCache::new(),
            &CacheMetricsStore::new(),
        );

        assert_eq!(snapshot.active_connections.len(), 1);
        assert_eq!(snapshot.active_connections[0].model_name, "qwen-3b");
        assert_eq!(snapshot.total_requests, 1);
        assert_eq!(snapshot.recent_requests.len(), 1);
    }

    #[test]
    fn build_reports_available_slots() {
        let connections = ActiveConnectionsRegistry::new();
        let slots = SlotsCache::new();
        slots.set(SlotsPollResult::Available(vec![]));
        let metrics = ContextMetricsStore::new();
        let upstream_health = UpstreamHealth::new();

        let snapshot = DashboardSnapshot::build(
            &connections,
            &slots,
            &metrics,
            &upstream_health,
            &CacheStatusCache::new(),
            &CacheMetricsStore::new(),
        );

        assert!(snapshot.slots_available);
        assert!(snapshot.slots_status.is_none());
    }

    #[test]
    fn snapshot_is_serializable_regardless_of_slots_state() {
        // Regression guard: `SlotsPollResult` cannot be serialized directly
        // as an internally-tagged enum (a newtype variant containing a
        // `Vec` cannot carry an injected tag key). `DashboardSnapshot`
        // flattens it into `slots_available`/`slots`/`slots_status`
        // instead, which must always serialize cleanly.
        for result in [
            SlotsPollResult::Available(vec![]),
            SlotsPollResult::Disabled,
            SlotsPollResult::Unreachable("boom".to_string()),
        ] {
            let connections = ActiveConnectionsRegistry::new();
            let slots = SlotsCache::new();
            slots.set(result);
            let metrics = ContextMetricsStore::new();
            let upstream_health = UpstreamHealth::new();
            let snapshot = DashboardSnapshot::build(
                &connections,
                &slots,
                &metrics,
                &upstream_health,
                &CacheStatusCache::new(),
                &CacheMetricsStore::new(),
            );

            serde_json::to_string(&snapshot).expect("DashboardSnapshot must always serialize");
        }
    }

    #[tokio::test]
    async fn publisher_shuts_down_promptly_on_cancellation() {
        let cancel = CancellationToken::new();
        let handle = spawn_dashboard_publisher(empty_state(), cancel.clone());

        cancel.cancel();

        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("publisher did not shut down promptly after cancellation")
            .expect("publisher task panicked");
    }

    /// Exercises the actual publisher loop end-to-end against real time,
    /// rather than a `start_paused` virtual clock: `tokio::time::advance`
    /// only fires due timers, it does not guarantee the woken publisher
    /// task is polled to completion (including its `broadcaster.send`)
    /// before the test's own timeout future is polled, so a paused-clock
    /// version of this test was flaky. `PUBLISH_INTERVAL` is 1s, so this
    /// test takes a little over a second — acceptable for a single
    /// integration-style test of the publish loop's wiring.
    #[tokio::test]
    async fn publisher_pushes_a_snapshot_after_the_first_interval() {
        let state = empty_state();
        let cancel = CancellationToken::new();
        let stream = state.broadcaster.subscribe_events();
        tokio::pin!(stream);

        let handle = spawn_dashboard_publisher(Arc::clone(&state), cancel.clone());

        let snapshot =
            tokio::time::timeout(PUBLISH_INTERVAL + Duration::from_secs(1), stream.next())
                .await
                .expect("no snapshot published before timeout")
                .expect("broadcaster stream ended unexpectedly");
        assert_eq!(snapshot.total_requests, 0);

        cancel.cancel();
        handle.await.expect("publisher task panicked");
    }

    // ── CacheStatus ──────────────────────────────────────────────────────

    #[test]
    fn healthy_cache_raises_nothing() {
        let s = CacheStatus::build(true, true, CacheRamHealth::Healthy { mb: 70_008 });
        assert_eq!(s.ram_state, "healthy");
        assert_eq!(s.ram_budget_mb, Some(70_008));
        assert!(!s.needs_attention);
        assert!(s.warnings.is_empty());
        assert!(!s.disk_suppressed_for_model);
    }

    #[test]
    fn low_budget_warns_and_names_the_figure() {
        let s = CacheStatus::build(true, true, CacheRamHealth::Low { mb: 2048 });
        assert_eq!(s.ram_state, "low");
        assert!(s.needs_attention);
        assert_eq!(s.warnings.len(), 1);
        assert!(s.warnings[0].contains("2048"), "{:?}", s.warnings);
    }

    /// A budget the user switched off is not a fault — warning about it would
    /// be nagging someone about their own setting.
    #[test]
    fn user_disabled_budget_is_silent_but_forced_one_is_not() {
        let chosen = CacheStatus::build(true, true, CacheRamHealth::DisabledByUser);
        assert_eq!(chosen.ram_state, "disabled_by_user");
        assert!(!chosen.needs_attention);
        assert!(chosen.warnings.is_empty());

        let forced = CacheStatus::build(true, true, CacheRamHealth::DisabledInsufficientRam);
        assert_eq!(forced.ram_state, "disabled_insufficient_ram");
        assert!(forced.needs_attention);
        assert_eq!(forced.warnings.len(), 1);
    }

    #[test]
    fn partial_kv_model_reports_the_disk_layer_as_suppressed() {
        let s = CacheStatus::build(true, false, CacheRamHealth::Healthy { mb: 70_008 });
        assert!(s.disk_suppressed_for_model);
        assert!(s.needs_attention);
        assert_eq!(s.warnings.len(), 1);
        assert!(s.warnings[0].contains("Disk cache"), "{:?}", s.warnings);
    }

    /// With the disk layer switched off proxy-wide there is nothing to
    /// suppress, so an unsupported model must not produce a warning about a
    /// feature the user isn't using.
    #[test]
    fn disk_disabled_proxy_wide_suppresses_nothing() {
        let s = CacheStatus::build(false, false, CacheRamHealth::Healthy { mb: 70_008 });
        assert!(!s.disk_suppressed_for_model);
        assert!(!s.needs_attention);
        assert!(s.warnings.is_empty());
    }

    /// The worst case — a cramped cache *and* no disk fallback — must surface
    /// both causes, since fixing only one leaves the user still slow.
    #[test]
    fn low_ram_hybrid_model_reports_both_causes() {
        let s = CacheStatus::build(true, false, CacheRamHealth::Low { mb: 1024 });
        assert!(s.needs_attention);
        assert_eq!(s.warnings.len(), 2, "{:?}", s.warnings);
    }

    #[test]
    fn llama_default_reports_no_budget_and_no_warning() {
        let s = CacheStatus::build(true, true, CacheRamHealth::LlamaDefault);
        assert_eq!(s.ram_state, "llama_default");
        assert_eq!(s.ram_budget_mb, None);
        assert!(!s.needs_attention);
    }

    // ── CacheStatusCache ─────────────────────────────────────────────────

    #[test]
    fn cache_starts_empty_and_records_the_latest_status() {
        let cache = CacheStatusCache::new();
        assert_eq!(cache.get(), None, "nothing resolved yet");

        let healthy = CacheStatus::build(true, true, CacheRamHealth::Healthy { mb: 8192 });
        cache.set(healthy.clone());
        assert_eq!(cache.get(), Some(healthy));

        // A model swap replaces it rather than accumulating.
        let low = CacheStatus::build(true, false, CacheRamHealth::Low { mb: 1024 });
        cache.set(low.clone());
        assert_eq!(cache.get(), Some(low));
    }

    /// The snapshot must expose whatever the cache holds, so the request path
    /// and the publisher agree without further plumbing.
    #[test]
    fn snapshot_surfaces_the_recorded_cache_status() {
        let connections = ActiveConnectionsRegistry::new();
        let slots = SlotsCache::new();
        let metrics = ContextMetricsStore::new();
        let upstream_health = UpstreamHealth::new();
        let cache = CacheStatusCache::new();
        let cache_metrics = CacheMetricsStore::new();

        let before = DashboardSnapshot::build(
            &connections,
            &slots,
            &metrics,
            &upstream_health,
            &cache,
            &cache_metrics,
        );
        assert_eq!(before.cache, None);

        cache.set(CacheStatus::build(
            true,
            false,
            CacheRamHealth::Low { mb: 1024 },
        ));
        let after = DashboardSnapshot::build(
            &connections,
            &slots,
            &metrics,
            &upstream_health,
            &cache,
            &cache_metrics,
        );
        let status = after.cache.expect("cache status present after set");
        assert!(status.needs_attention);
        assert_eq!(status.warnings.len(), 2);
    }

    /// Reuse totals must come from the live store at snapshot time, not from
    /// whatever was frozen into the cached config — otherwise the figure would
    /// only move when a model swapped.
    #[test]
    fn snapshot_reads_usage_live_rather_than_from_the_cached_config() {
        let connections = ActiveConnectionsRegistry::new();
        let slots = SlotsCache::new();
        let metrics = ContextMetricsStore::new();
        let upstream_health = UpstreamHealth::new();
        let cache = CacheStatusCache::new();
        let cache_metrics = CacheMetricsStore::new();

        // Config recorded once, as the request path does on model resolution.
        cache.set(CacheStatus::build(
            true,
            true,
            CacheRamHealth::Healthy { mb: 70_008 },
        ));

        let build = |cm: &CacheMetricsStore| {
            DashboardSnapshot::build(&connections, &slots, &metrics, &upstream_health, &cache, cm)
                .cache
                .expect("cache status present")
        };

        assert_eq!(build(&cache_metrics).usage, CacheUsage::default());

        // Requests land *after* the config was cached; the snapshot must
        // still pick them up.
        cache_metrics.record(10_000, Some(9_500));
        let got = build(&cache_metrics);
        assert_eq!(got.usage.reporting_requests, 1);
        assert_eq!(got.usage.cached_tokens, 9_500);
        assert_eq!(got.usage.last_prompt_tokens, Some(10_000));

        // And the stored config is untouched by that — it still compares
        // equal to a freshly built one, which is what lets `set` skip
        // redundant writes on every subsequent request.
        assert_eq!(
            cache.get().expect("config cached"),
            CacheStatus::build(true, true, CacheRamHealth::Healthy { mb: 70_008 }),
        );
    }
}
