//! Integration tests for `POST /v1/proxy/cache/clear`.
//!
//! Spawns the real `gglib_proxy::serve` (not a hand-rolled router), following
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
    ApprovalDecision, CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError,
    ModelRuntimePort, ModelSummary, RepositoryError, RunningTarget, SettingsRepository,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, NoopEmitter};
use gglib_mcp::McpService;
use gglib_proxy::CouncilDeps;
use tokio::sync::{mpsc, oneshot};

// ─── Mock ports (trimmed to the bare minimum needed to boot `serve`) ──────

#[derive(Debug)]
struct NoopRunner;
#[async_trait]
impl gglib_proxy::CouncilRunnerPort for NoopRunner {
    async fn run(
        &self,
        _: &str,
        _: gglib_proxy::CouncilRunParams,
        _: mpsc::Sender<CouncilEvent>,
        _: CancellationToken,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

struct NoopApprovalRegistry;
impl gglib_core::ports::CouncilApprovalRegistryPort for NoopApprovalRegistry {
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
impl gglib_core::ports::CouncilRepositoryPort for NoopOrchestratorRepo {
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
        Ok(RunningTarget::local(0, 1, "mock".into(), 4096, false))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

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

/// Spawn the real `gglib_proxy::serve` with the given cache settings.
/// Returns `(proxy_base_url, cancel_token)`.
async fn spawn_proxy(
    cache_enabled: bool,
    slot_dir: Option<std::path::PathBuf>,
) -> (String, CancellationToken) {
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
            cache_enabled,
            slot_dir,
        )
        .await
        .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

// ─── Tests ──────────────────────────────────────────────────────────────

/// When cache is disabled, the handler should return 200 with a "skipped"
/// status — it's an idempotent no-op rather than an error.
#[tokio::test]
async fn cache_clear_returns_skipped_when_cache_disabled() {
    let (base_url, cancel) = spawn_proxy(false, None).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert_eq!(body["status"], "skipped");
    assert!(
        body["message"]
            .as_str()
            .expect("message is string")
            .contains("cache not enabled")
    );

    cancel.cancel();
}

/// A session ID that looks like a path-traversal attempt should be rejected
/// with 400 Bad Request before any filesystem access.
#[tokio::test]
async fn cache_clear_returns_400_on_invalid_session_id() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .header("x-gglib-session-id", "../etc/passwd")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert!(
        body["error"]
            .as_str()
            .expect("error is string")
            .contains("invalid session id")
    );

    cancel.cancel();
}

/// Without a session header the handler should clear all slots and return
/// 200 with an "all slots cleared" message.
#[tokio::test]
async fn cache_clear_clears_all_when_no_session_id() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert_eq!(body["status"], "ok");
    assert!(
        body["message"]
            .as_str()
            .expect("message is string")
            .contains("all slots cleared")
    );

    cancel.cancel();
}

/// With a valid session ID the handler should return 200 and report that a
/// session was cleared (the message differs from the "all slots" path).
#[tokio::test]
async fn cache_clear_populates_per_session_cleared() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .header("x-gglib-session-id", "test-session-abc")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert_eq!(body["status"], "ok");
    // The handler returns "session cleared" when a session ID is provided,
    // distinguishing it from the "all slots cleared" path.
    assert!(
        body["message"]
            .as_str()
            .expect("message is string")
            .contains("session cleared")
    );

    cancel.cancel();
}

/// When cache is enabled but no slot directory was configured, the handler
/// should return 500 Internal Server Error.
#[tokio::test]
async fn cache_clear_returns_500_when_slot_dir_missing() {
    let (base_url, cancel) = spawn_proxy(true, None).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert!(
        body["error"]
            .as_str()
            .expect("error is string")
            .contains("slot_dir not configured")
    );

    cancel.cancel();
}
