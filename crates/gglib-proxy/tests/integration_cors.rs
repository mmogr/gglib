//! Verifies the proxy's permissive CORS layer, added specifically so the
//! Tauri GUI's webview (origin `tauri://localhost` / `http://tauri.localhost`
//! on Windows) can call this proxy's endpoints — including opening an
//! `EventSource` connection to `GET /v1/proxy/status/stream` — without the
//! browser blocking the request as cross-origin.
//!
//! Uses the real `gglib_proxy::serve` (not a hand-rolled router), following
//! the same self-contained-harness pattern as the other integration tests in
//! this crate (each file duplicates its own minimal mocks rather than
//! sharing a `tests/common` module).

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::Settings;
use gglib_core::domain::council::{CouncilEvent, CouncilRun, CouncilRunEvent, CouncilRunStatus};
use gglib_core::ports::{
    ApprovalDecision, CatalogError, CouncilApprovalRegistryPort, CouncilRepositoryPort,
    ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort, ModelSummary,
    RepositoryError, RunningTarget, SettingsRepository,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, NoopEmitter};
use gglib_mcp::McpService;
use gglib_proxy::{CouncilDeps, CouncilRunParams, CouncilRunnerPort};
use tokio::sync::{mpsc, oneshot};

// ─── Mock ports (trimmed to the bare minimum needed to boot `serve`) ──────

#[derive(Debug)]
struct NoopRunner;
#[async_trait]
impl CouncilRunnerPort for NoopRunner {
    async fn run(
        &self,
        _: &str,
        _: CouncilRunParams,
        _: mpsc::Sender<CouncilEvent>,
        _: CancellationToken,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
struct NoopApprovalRegistry;
impl CouncilApprovalRegistryPort for NoopApprovalRegistry {
    fn register(&self, _: String, _: oneshot::Sender<ApprovalDecision>) {}
    fn resolve(&self, _: &str, _: ApprovalDecision) -> bool {
        false
    }
    fn is_pending(&self, _: &str) -> bool {
        false
    }
}
struct NoopOrchestratorRepo;
#[async_trait]
impl CouncilRepositoryPort for NoopOrchestratorRepo {
    async fn create_run(&self, _: CouncilRun) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update_run_status(&self, _: &str, _: CouncilRunStatus) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update_graph(&self, _: &str, _: &str) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn append_event(&self, _: CouncilRunEvent) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn get_run(&self, _: &str) -> Result<Option<CouncilRun>, RepositoryError> {
        Ok(None)
    }
    async fn list_runs(
        &self,
        _: Option<CouncilRunStatus>,
    ) -> Result<Vec<CouncilRun>, RepositoryError> {
        Ok(vec![])
    }
    async fn list_events(&self, _: &str) -> Result<Vec<CouncilRunEvent>, RepositoryError> {
        Ok(vec![])
    }
    async fn truncate_events_after_wave(&self, _: &str, _: u32) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn mark_interrupted_runs(&self) -> Result<u64, RepositoryError> {
        Ok(0)
    }
}
fn make_orchestrator_deps() -> CouncilDeps {
    CouncilDeps {
        runner: Arc::new(NoopRunner),
        approval_registry: Arc::new(NoopApprovalRegistry),
        council_repo: Arc::new(NoopOrchestratorRepo),
    }
}

struct MockSettingsRepo;
#[async_trait]
impl SettingsRepository for MockSettingsRepo {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(Settings::with_defaults())
    }
    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Runtime port that never actually launches anything — no test here
/// exercises `/v1/chat/completions`, so `ensure_model_running` is never
/// called in practice.
#[derive(Debug)]
struct NoopRuntime;
#[async_trait]
impl ModelRuntimePort for NoopRuntime {
    async fn ensure_model_running(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Ok(RunningTarget::local(0, 1, "mock".into(), 4096))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Catalog port with no models — none of these tests call `/v1/models`.
#[derive(Debug)]
struct EmptyCatalog;
#[async_trait]
impl ModelCatalogPort for EmptyCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![])
    }
    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

struct EmptyMcpRepo;
#[async_trait]
impl McpServerRepository for EmptyMcpRepo {
    async fn insert(&self, _s: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("not implemented".into()))
    }
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.into()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(vec![])
    }
    async fn update(&self, _s: &McpServer) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
}

// ─── Proxy harness ─────────────────────────────────────────────────────────

/// Spawn the real `gglib_proxy::serve` with no upstream configured (not
/// needed — these tests only exercise `/v1/proxy/status`, which doesn't
/// touch the runtime/catalog ports). Returns `(proxy_base_url, cancel)`.
async fn spawn_proxy() -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);
    let mcp = Arc::new(McpService::new(
        Arc::new(EmptyMcpRepo),
        Arc::new(NoopEmitter::new()),
    ));

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            mcp,
            make_orchestrator_deps(),
            cancel_clone,
            Arc::new(MockSettingsRepo),
        )
        .await
        .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

// ─── Tests ──────────────────────────────────────────────────────────────

/// A plain GET from a Tauri-webview origin must come back with
/// `access-control-allow-origin` set — this is exactly what an
/// `EventSource` connection to `/v1/proxy/status/stream` needs, since
/// `EventSource` performs a simple GET (no preflight) but the browser still
/// enforces CORS on the response.
#[tokio::test]
async fn get_request_from_tauri_origin_receives_cors_header() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .get(format!("{base_url}/v1/proxy/status"))
        .header("Origin", "tauri://localhost")
        .send()
        .await
        .expect("request should succeed");

    assert!(resp.status().is_success());
    let allow_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .expect("missing access-control-allow-origin header")
        .to_str()
        .unwrap();
    // `Any` reflects as a literal `*` (no credentials involved), which is
    // valid for a non-credentialed request from any origin, including
    // `tauri://localhost`.
    assert_eq!(allow_origin, "*");

    cancel.cancel();
}

/// A CORS preflight (`OPTIONS` with `Access-Control-Request-Method`) against
/// the SSE endpoint must succeed and carry the CORS response headers, which
/// is what a browser sends before allowing the actual `EventSource`/fetch
/// call through.
#[tokio::test]
async fn preflight_request_to_sse_endpoint_is_allowed() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .request(
            reqwest::Method::OPTIONS,
            format!("{base_url}/v1/proxy/status/stream"),
        )
        .header("Origin", "tauri://localhost")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("preflight request should succeed");

    assert!(
        resp.status().is_success(),
        "preflight should not be rejected, got {}",
        resp.status()
    );
    assert!(
        resp.headers().contains_key("access-control-allow-origin"),
        "preflight response missing access-control-allow-origin"
    );
    assert!(
        resp.headers().contains_key("access-control-allow-methods"),
        "preflight response missing access-control-allow-methods"
    );

    cancel.cancel();
}

/// A request from a plain `http://localhost:5173` (Vite dev server) origin
/// works identically — the permissive layer doesn't special-case Tauri.
#[tokio::test]
async fn get_request_from_vite_dev_origin_receives_cors_header() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .get(format!("{base_url}/v1/proxy/status"))
        .header("Origin", "http://localhost:5173")
        .send()
        .await
        .expect("request should succeed");

    assert!(resp.status().is_success());
    assert!(resp.headers().contains_key("access-control-allow-origin"));

    cancel.cancel();
}
