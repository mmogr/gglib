//! `/v1/models` and the dashboard on the pinned (`gglib serve`) path.
//!
//! Pinned mode refuses every model but one. These tests pin down the
//! consequence for the catalog endpoint: it must advertise what the proxy
//! will actually accept, and nothing else. A BYOK client that cannot switch
//! models — VS Code Copilot being the motivating case — builds its picker
//! from this list once, so an entry that can only come back as
//! `PinnedModelMismatch` is worse than no entry at all.
//!
//! The pinned *guard* is tested in `gglib-runtime`; the mock here only
//! reports pinning, so a failure means the catalog is wrong rather than the
//! enforcement.

mod fixtures;

use std::sync::Arc;
use std::time::Duration;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort, SettingsRepository};
use reqwest::Client;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use fixtures::common::{
    MockSettingsRepo, NoopRuntime, PinnedRuntime, ProfileSettingsRepo, StaticCatalog,
    make_mcp_service, make_orchestrator_deps,
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
            make_orchestrator_deps(),
            proxy_cancel,
            settings,
            None, // inference_override
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            &gglib_core::CorsConfig::LocalOnly,
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

/// Council runs dispatch to whatever model is loaded, which under pinning is
/// the pinned model — so the virtuals stay servable and stay listed.
#[tokio::test]
async fn pinned_proxy_still_advertises_the_council_virtuals() {
    let (base, cancel) = spawn(
        Arc::new(PinnedRuntime(PINNED)),
        Arc::new(StaticCatalog::new(&[PINNED, FOREIGN])),
        Arc::new(MockSettingsRepo),
    )
    .await;

    let ids = model_ids(&base).await;

    for virtual_model in ["gglib-council", "gglib-council:interactive"] {
        assert!(
            ids.contains(&virtual_model.to_owned()),
            "pinning dropped {virtual_model}, which it can still serve: {ids:?}"
        );
    }

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
