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

use gglib_sse::Broadcaster;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::connections::{ActiveConnectionSnapshot, ActiveConnectionsRegistry};
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::slots::{SlotSnapshot, SlotsPollResult};
use crate::slots_poller::SlotsCache;
use crate::upstream_health::{UpstreamHealth, UpstreamHealthSnapshot};

/// Number of recent request snapshots included in each [`DashboardSnapshot`].
const RECENT_REQUEST_LIMIT: usize = 20;

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
    /// Whether KV cache persistence is enabled on this proxy instance.
    pub cache_enabled: bool,
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
        cache_enabled: bool,
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
            cache_enabled,
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
    /// Whether KV cache persistence is enabled.
    pub cache_enabled: bool,
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
        cache_enabled: bool,
    ) -> Self {
        Self {
            connections,
            slots,
            metrics,
            upstream_health,
            broadcaster: Arc::new(Broadcaster::new(BROADCAST_CAPACITY)),
            cache_enabled,
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
            self.cache_enabled,
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
            false,
        ))
    }

    #[test]
    fn build_aggregates_empty_stores() {
        let connections = ActiveConnectionsRegistry::new();
        let slots = SlotsCache::new();
        let metrics = ContextMetricsStore::new();
        let upstream_health = UpstreamHealth::new();

        let snapshot =
            DashboardSnapshot::build(&connections, &slots, &metrics, &upstream_health, false);

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
        let snapshot =
            DashboardSnapshot::build(&connections, &slots, &metrics, &upstream_health, false);

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

        let snapshot =
            DashboardSnapshot::build(&connections, &slots, &metrics, &upstream_health, false);

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
            let snapshot =
                DashboardSnapshot::build(&connections, &slots, &metrics, &upstream_health, false);

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
}
