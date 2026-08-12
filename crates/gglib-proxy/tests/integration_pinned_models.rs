//! `/v1/models` and the dashboard on the pinned (`gglib serve`) path.
//!
//! Pinned mode refuses every model but one. These tests pin down the
//! consequence for the catalog endpoint: it must advertise what the proxy
//! will actually accept, and nothing else. A BYOK client that cannot switch
//! models — VS Code Copilot being the motivating case — builds its picker
//! from this list once, so an entry that can only come back as
//! `PinnedModelMismatch` is worse than no entry at all.
//!
//! The pinned guard itself is unit-tested in `gglib-runtime`'s
//! `manager.rs`/`swap_state.rs`. Most tests below use [`PinnedRuntime`],
//! which only *reports* pinning, so a failure there means the catalog is
//! wrong rather than the enforcement. The enforcement test near the bottom
//! uses [`EnforcingPinnedRuntime`] instead, to assert the actual wire
//! contract a BYOK client hits — 404 plus `pinned_model_mismatch` — over
//! real HTTP.

mod fixtures;

use std::sync::Arc;
use std::time::Duration;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort, SettingsRepository};
use reqwest::Client;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use fixtures::common::{
    EnforcingPinnedRuntime, MockSettingsRepo, NoopRuntime, PinnedRuntime, ProfileSettingsRepo,
    StaticCatalog, make_mcp_service,
};

const PINNED: &str = "qwen2.5";
const FOREIGN: &str = "llama-3-8b";

/// Spawn a proxy over the given runtime, catalog and settings.
async fn spawn(
    runtime: Arc<dyn ModelRuntimePort>,
    catalog: Arc<dyn ModelCatalogPort>,
    settings: Arc<dyn SettingsRepository>,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let proxy_cancel = cancel.clone();

    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            make_mcp_service(),
            proxy_cancel,
            settings,
            None, // inference_override
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            Arc::new(gglib_core::domain::defects::ModelDefectLedger::new()),
            &gglib_core::ProxyAccessConfig::default(),
        )
        .await
        .ok();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (format!("http://{addr}"), cancel)
}

/// The `id` of every entry `/v1/models` advertised.
async fn model_ids(base: &str) -> Vec<String> {
    let resp = Client::new()
        .get(format!("{base}/v1/models"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_owned))
        .collect()
}

// ─── Catalog filtering ────────────────────────────────────────────────────

/// The whole point: a pinned proxy must not advertise models it will refuse.
#[tokio::test]
async fn pinned_proxy_advertises_only_the_pinned_model() {
    let (base, cancel) = spawn(
        Arc::new(PinnedRuntime(PINNED)),
        Arc::new(StaticCatalog::new(&[PINNED, FOREIGN, "mistral-7b"])),
        Arc::new(MockSettingsRepo),
    )
    .await;

    let ids = model_ids(&base).await;

    assert!(ids.contains(&PINNED.to_owned()), "pinned model missing");
    assert!(
        !ids.iter().any(|id| id.starts_with(FOREIGN)),
        "foreign model advertised on a pinned endpoint: {ids:?}"
    );
    assert!(
        !ids.iter().any(|id| id.starts_with("mistral-7b")),
        "foreign model advertised on a pinned endpoint: {ids:?}"
    );

    cancel.cancel();
}

/// Filtering must not leak into the ordinary proxy, whose entire job is to
/// offer the catalog and swap on demand.
#[tokio::test]
async fn unpinned_proxy_advertises_the_whole_catalog() {
    let (base, cancel) = spawn(
        Arc::new(NoopRuntime),
        Arc::new(StaticCatalog::new(&[PINNED, FOREIGN])),
        Arc::new(MockSettingsRepo),
    )
    .await;

    let ids = model_ids(&base).await;

    assert!(
        ids.contains(&PINNED.to_owned()),
        "missing {PINNED}: {ids:?}"
    );
    assert!(
        ids.contains(&FOREIGN.to_owned()),
        "unpinned proxy dropped {FOREIGN}: {ids:?}"
    );

    cancel.cancel();
}

/// A profile changes the request body, never which model runs, so variants
/// of the pinned model are servable and must survive the filter — while
/// variants of a foreign model must not appear at all.
#[tokio::test]
async fn pinned_proxy_keeps_variants_of_the_pinned_model_only() {
    let (base, cancel) = spawn(
        Arc::new(PinnedRuntime(PINNED)),
        Arc::new(StaticCatalog::new(&[PINNED, FOREIGN])),
        Arc::new(ProfileSettingsRepo("coding")),
    )
    .await;

    let ids = model_ids(&base).await;

    assert!(
        ids.contains(&format!("{PINNED}:coding")),
        "pinned model's profile variant was filtered out: {ids:?}"
    );
    assert!(
        !ids.contains(&format!("{FOREIGN}:coding")),
        "foreign model's profile variant advertised: {ids:?}"
    );

    cancel.cancel();
}

/// An empty catalog must not resurrect the filtered models — the pinned name
/// is a filter, not a synthesized entry.
#[tokio::test]
async fn pinned_proxy_advertises_nothing_when_the_model_is_absent() {
    let (base, cancel) = spawn(
        Arc::new(PinnedRuntime(PINNED)),
        Arc::new(StaticCatalog::new(&[FOREIGN])),
        Arc::new(MockSettingsRepo),
    )
    .await;

    let ids = model_ids(&base).await;

    assert!(
        !ids.contains(&FOREIGN.to_owned()),
        "foreign model survived with the pinned model absent: {ids:?}"
    );

    cancel.cancel();
}

// ─── Dashboard ────────────────────────────────────────────────────────────

/// `serve` runs the proxy stack, so it gets the dashboard — the observability
/// gap that motivated epic #630. Asserts the contract clients read, not just
/// a 200.
#[tokio::test]
async fn pinned_proxy_serves_the_dashboard() {
    let (base, cancel) = spawn(
        Arc::new(PinnedRuntime(PINNED)),
        Arc::new(StaticCatalog::new(&[PINNED])),
        Arc::new(MockSettingsRepo),
    )
    .await;

    let resp = Client::new()
        .get(format!("{base}/v1/proxy/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let snapshot: Value = resp.json().await.unwrap();
    for field in [
        "active_connections",
        "slots",
        "total_requests",
        "upstream_health",
    ] {
        assert!(
            snapshot.get(field).is_some(),
            "dashboard snapshot missing {field}: {snapshot}"
        );
    }

    cancel.cancel();
}

// ─── Enforcement ──────────────────────────────────────────────────────────

/// The contract a BYOK client hits when it asks for the wrong model: a 404
/// naming both models, distinguishable from a plain `model_not_found` so the
/// client can tell "no such model anywhere" from "not on this endpoint".
///
/// Everything above this test proves the *catalog* never offers a foreign
/// model; this proves that if a client asks anyway — a stale cache, a
/// hand-rolled request — the refusal itself is correct over real HTTP, not
/// just at the `SwapState`/`ErrorResponse` unit level.
#[tokio::test]
async fn pinned_proxy_refuses_a_foreign_model_over_http() {
    let (base, cancel) = spawn(
        Arc::new(EnforcingPinnedRuntime(PINNED)),
        Arc::new(StaticCatalog::new(&[PINNED, FOREIGN])),
        Arc::new(MockSettingsRepo),
    )
    .await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&serde_json::json!({
            "model": FOREIGN,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        404,
        "a pinned endpoint must refuse a foreign model"
    );

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "pinned_model_mismatch");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains(PINNED),
        "message should name the pinned model: {message}"
    );
    assert!(
        message.contains(FOREIGN),
        "message should name the requested model: {message}"
    );

    cancel.cancel();
}

/// The pinned model itself must still be servable through the same
/// enforcing runtime — the guard rejects on identity, not universally.
#[tokio::test]
async fn pinned_proxy_still_admits_the_pinned_model_over_http() {
    let (base, cancel) = spawn(
        Arc::new(EnforcingPinnedRuntime(PINNED)),
        Arc::new(StaticCatalog::new(&[PINNED, FOREIGN])),
        Arc::new(MockSettingsRepo),
    )
    .await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&serde_json::json!({
            "model": PINNED,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .unwrap();

    // `EnforcingPinnedRuntime` has no real upstream to forward to, so this
    // proves the request got *past* the pinned guard (anything other than
    // the mismatch's 404/pinned_model_mismatch), not that it fully succeeds.
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_ne!(
        body["error"]["code"], "pinned_model_mismatch",
        "the pinned model itself must not be rejected by the pin guard, got {status}: {body}"
    );

    cancel.cancel();
}
