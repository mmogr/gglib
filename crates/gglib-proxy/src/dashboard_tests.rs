//! Tests for the dashboard SSE surface.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

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
        Arc::new(CacheMetricsStore::new()),
        Arc::new(gglib_core::ports::NoopModelRuntime),
        Arc::new(SamplingAuditStore::new()),
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
        &CacheMetricsStore::new(),
        &LaunchNarrationCache::new(),
        None,
        &SamplingAuditStore::new(),
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
        dialect_residue: false,
        tool_repaired: false,
        seq: 0,
        model_name: "qwen-3b".to_string(),
        payload_chars_before: 100,
        payload_chars_after: 100,
        messages_truncated: 0,
        was_clamped: false,
        grammar_enforced: false,
        loop_guard_tripped: false,
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
        &CacheMetricsStore::new(),
        &LaunchNarrationCache::new(),
        None,
        &SamplingAuditStore::new(),
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
        &CacheMetricsStore::new(),
        &LaunchNarrationCache::new(),
        None,
        &SamplingAuditStore::new(),
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
            &CacheMetricsStore::new(),
            &LaunchNarrationCache::new(),
            None,
            &SamplingAuditStore::new(),
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

    let snapshot = tokio::time::timeout(PUBLISH_INTERVAL + Duration::from_secs(1), stream.next())
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

#[test]
fn launch_cache_starts_empty_and_records_the_latest_narration() {
    use gglib_core::domain::{LaunchDecision, LaunchNarration};

    let cache = LaunchNarrationCache::new();
    assert_eq!(cache.get(), None, "nothing launched yet");

    let mut first = LaunchNarration::new("qwen3", Some("Q4_K_M".to_string()), 1_073_741_824);
    first.push(LaunchDecision::new("ctx", "32768", "model server_defaults"));
    cache.set(first.clone());
    assert_eq!(cache.get(), Some(first));

    // A model swap replaces the narration rather than accumulating.
    let second = LaunchNarration::new("llama3", None, 0);
    cache.set(second.clone());
    assert_eq!(cache.get(), Some(second));
}

/// The distinction the whole Tier C liveness contract rests on, asserted
/// at the wire boundary rather than only in the type: a blind organ and a
/// clean one must not serialize to the same thing, or every consumer will
/// render them the same and the "unknown means nobody knows" discipline
/// dies at the JSON layer.
#[test]
fn a_blind_sampling_audit_serializes_differently_from_a_clean_one() {
    let blind = SamplingAuditStore::new();
    blind.mark_blind("llama-server was launched with --no-slots");

    let clean = SamplingAuditStore::new();
    clean.record_poll(&crate::sampling_audit::PollOutcome {
        comparisons: 40,
        divergences: 0,
        skipped_ambiguous: 0,
        found: Vec::new(),
    });

    let render = |store: &SamplingAuditStore| {
        serde_json::to_string(&store.snapshot()).expect("snapshot serializes")
    };
    let blind_json = render(&blind);
    let clean_json = render(&clean);

    assert_ne!(blind_json, clean_json);
    assert!(blind_json.contains("\"state\":\"blind\""), "{blind_json}");
    assert!(
        blind_json.contains("--no-slots"),
        "blindness must carry its reason: {blind_json}"
    );
    assert!(
        clean_json.contains("\"state\":\"comparing\""),
        "{clean_json}"
    );
    assert!(clean_json.contains("\"divergences\":0"), "{clean_json}");
}

/// The baseline half's version of the rule above. A `/props` read that was
/// attempted and failed must not serialize the same as one the poller has
/// not reached yet — otherwise the panel says "not read yet" about a read
/// that happened, and the cause never reaches anyone.
#[test]
fn an_unreadable_baseline_serializes_differently_from_an_unread_one() {
    let unread = SamplingAuditStore::new();

    let failed = SamplingAuditStore::new();
    failed.set_baseline(crate::props::BaselineState::Unreadable {
        reason: "connection refused".to_string(),
    });

    let render = |store: &SamplingAuditStore| {
        serde_json::to_string(&store.snapshot()).expect("snapshot serializes")
    };
    let unread_json = render(&unread);
    let failed_json = render(&failed);

    assert_ne!(unread_json, failed_json);
    assert!(
        unread_json.contains("\"state\":\"not_yet_read\""),
        "{unread_json}"
    );
    assert!(
        failed_json.contains("\"state\":\"unreadable\""),
        "{failed_json}"
    );
    assert!(
        failed_json.contains("connection refused"),
        "an unreadable baseline must carry its cause: {failed_json}"
    );
}

/// The store the poller writes must be the store the snapshot reads —
/// two `Arc`s of the same thing, not two stores.
#[test]
fn the_snapshot_reads_the_same_audit_store_the_poller_writes() {
    let state = empty_state();
    assert_eq!(
        state.snapshot().sampling_audit.state,
        crate::sampling_audit::AuditState::NotYetObserved
    );

    state.sampling_audit.mark_blind("upstream unreachable");

    match state.snapshot().sampling_audit.state {
        crate::sampling_audit::AuditState::Blind { reason } => {
            assert_eq!(reason, "upstream unreachable");
        }
        other => panic!("expected Blind, got {other:?}"),
    }
}

/// Tier 1 parity: what the banner prints must also be reachable over
/// `GET /v1/proxy/status`, which is this snapshot.
#[test]
fn snapshot_surfaces_the_recorded_launch_narration() {
    use gglib_core::domain::{LaunchDecision, LaunchNarration};

    let connections = ActiveConnectionsRegistry::new();
    let slots = SlotsCache::new();
    let metrics = ContextMetricsStore::new();
    let upstream_health = UpstreamHealth::new();
    let launch = LaunchNarrationCache::new();

    let build = |launch: &LaunchNarrationCache| {
        DashboardSnapshot::build(
            &connections,
            &slots,
            &metrics,
            &upstream_health,
            &CacheStatusCache::new(),
            &CacheMetricsStore::new(),
            &CacheMetricsStore::new(),
            launch,
            None,
            &SamplingAuditStore::new(),
        )
    };

    assert_eq!(build(&launch).launch, None, "nothing launched yet");

    let mut narration = LaunchNarration::new("qwen3", Some("Q4_K_M".to_string()), 0);
    narration.push(LaunchDecision::new("kv", "q8_0", "default"));
    launch.set(narration.clone());

    assert_eq!(build(&launch).launch, Some(narration));
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
        &CacheMetricsStore::new(),
        &LaunchNarrationCache::new(),
        None,
        &SamplingAuditStore::new(),
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
        &CacheMetricsStore::new(),
        &LaunchNarrationCache::new(),
        None,
        &SamplingAuditStore::new(),
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
        DashboardSnapshot::build(
            &connections,
            &slots,
            &metrics,
            &upstream_health,
            &cache,
            cm,
            &CacheMetricsStore::new(),
            &LaunchNarrationCache::new(),
            None,
            &SamplingAuditStore::new(),
        )
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

/// The agent population is reported alongside the proxied one and never
/// merged into it — and it surfaces even before any proxied request has
/// resolved a model (so `cache` is still `None`).
#[test]
fn agent_usage_is_a_separate_population_from_the_proxied_figure() {
    let connections = ActiveConnectionsRegistry::new();
    let slots = SlotsCache::new();
    let metrics = ContextMetricsStore::new();
    let upstream_health = UpstreamHealth::new();
    let cache = CacheStatusCache::new();
    let proxied = CacheMetricsStore::new();
    let agent = CacheMetricsStore::new();

    // Only the agent store records — e.g. an agent turn with no proxied
    // traffic and no model config resolved yet.
    agent.record(8_000, Some(7_600));

    let snap = DashboardSnapshot::build(
        &connections,
        &slots,
        &metrics,
        &upstream_health,
        &cache,
        &proxied,
        &agent,
        &LaunchNarrationCache::new(),
        None,
        &SamplingAuditStore::new(),
    );

    assert_eq!(snap.agent_usage.reporting_requests, 1);
    assert_eq!(snap.agent_usage.cached_tokens, 7_600);
    assert_eq!(snap.agent_usage.last_prompt_tokens, Some(8_000));
    // Proxied config never resolved, yet agent_usage still surfaced.
    assert_eq!(snap.cache, None);
}
