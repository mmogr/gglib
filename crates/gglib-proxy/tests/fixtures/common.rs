//! Shared mock implementations for gglib-proxy integration tests.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use gglib_core::Settings;
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::council::{CouncilEvent, CouncilRun, CouncilRunEvent, CouncilRunStatus};
use gglib_core::domain::inference_profile::InferenceProfile;
use gglib_core::ports::{
    ApprovalDecision, CatalogError, CouncilApprovalRegistryPort, CouncilRepositoryPort,
    ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort, ModelSummary,
    RepositoryError, RunningTarget, SettingsRepository,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, NoopEmitter};
use gglib_mcp::McpService;
use gglib_proxy::{CouncilDeps, CouncilRunParams, CouncilRunnerPort};

// ─── ModelRuntimePort mock ────────────────────────────────────────────────

/// Runtime port that never actually launches anything.
#[derive(Debug)]
pub struct NoopRuntime;

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

/// Runtime port that reports itself pinned to one model.
///
/// The read side of `gglib serve`: what a caller sees when the manager was
/// built with `ProcessManager::new_pinned`. It does not enforce the pin —
/// that guard lives in `gglib-runtime` and is tested there — so a test can
/// tell the difference between "not advertised" and "refused".
#[derive(Debug)]
pub struct PinnedRuntime(pub &'static str);

#[async_trait]
impl ModelRuntimePort for PinnedRuntime {
    async fn ensure_model_running(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Ok(RunningTarget::local(0, 1, self.0.into(), 4096, false))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned_model(&self) -> Option<&str> {
        Some(self.0)
    }
}

/// Runtime port that *enforces* the pin, rather than only reporting it.
///
/// The write side of `gglib serve`: [`PinnedRuntime`] above deliberately
/// lets a foreign request through so catalog tests can tell "not
/// advertised" from "refused". This one refuses, so the wire contract a
/// BYOK client actually hits — 404 plus `pinned_model_mismatch` — can be
/// asserted end to end over HTTP, not just at the `SwapState`/error-mapping
/// unit level (`gglib-runtime`'s `manager.rs`, `gglib-proxy`'s
/// `models_tests.rs`).
#[derive(Debug)]
pub struct EnforcingPinnedRuntime(pub &'static str);

#[async_trait]
impl ModelRuntimePort for EnforcingPinnedRuntime {
    async fn ensure_model_running(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        if model_name != self.0 {
            return Err(ModelRuntimeError::PinnedModelMismatch {
                expected: self.0.to_string(),
                requested: model_name.to_string(),
            });
        }
        Ok(RunningTarget::local(0, 1, self.0.into(), 4096, false))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned_model(&self) -> Option<&str> {
        Some(self.0)
    }
}

// ─── ModelCatalogPort mock ────────────────────────────────────────────────

/// Catalog port with no models.
#[derive(Debug)]
pub struct EmptyCatalog;

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

/// Catalog port over a fixed set of model names.
///
/// Names are all `/v1/models` filtering cares about, so everything else is
/// filled with plausible constants rather than made configurable.
#[derive(Debug)]
pub struct StaticCatalog(pub Vec<String>);

impl StaticCatalog {
    /// Build a catalog listing the given model names.
    pub fn new(names: &[&str]) -> Self {
        Self(names.iter().map(|n| (*n).to_string()).collect())
    }

    fn summary(id: u32, name: &str) -> ModelSummary {
        ModelSummary {
            id,
            name: name.to_string(),
            tags: vec![],
            capabilities: Default::default(),
            param_count: "7B".to_string(),
            quantization: Some("Q4_K_M".to_string()),
            architecture: Some("llama".to_string()),
            created_at: 0,
            file_size: 0,
            context_length: Some(8192),
            inference_defaults: None,
            server_defaults: None,
        }
    }
}

#[async_trait]
impl ModelCatalogPort for StaticCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(self
            .0
            .iter()
            .enumerate()
            .map(|(i, name)| Self::summary(u32::try_from(i).unwrap_or(0) + 1, name))
            .collect())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self
            .0
            .iter()
            .position(|n| n == name)
            .map(|i| Self::summary(u32::try_from(i).unwrap_or(0) + 1, name)))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

// ─── SettingsRepository mock ──────────────────────────────────────────────

/// Returns default settings; save is a no-op.
pub struct MockSettingsRepo;

#[async_trait]
impl SettingsRepository for MockSettingsRepo {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(Settings::with_defaults())
    }

    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Settings carrying one listed inference profile, so `/v1/models` emits
/// `{model}:{name}` variant entries.
pub struct ProfileSettingsRepo(pub &'static str);

#[async_trait]
impl SettingsRepository for ProfileSettingsRepo {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(Settings {
            inference_profiles: Some(vec![InferenceProfile {
                name: self.0.to_string(),
                description: None,
                config: InferenceConfig::default(),
                list_in_models: true,
            }]),
            ..Settings::with_defaults()
        })
    }

    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

// ─── Council mocks (verified against trait definitions) ───────────────────

/// No-op council runner — `run` immediately returns Ok.
#[derive(Debug)]
pub struct NoopRunner;

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

/// No-op approval registry — all operations are no-ops.
pub struct NoopApprovalRegistry;

impl CouncilApprovalRegistryPort for NoopApprovalRegistry {
    fn register(&self, _: String, _: oneshot::Sender<ApprovalDecision>) {}
    fn resolve(&self, _: &str, _: ApprovalDecision) -> bool {
        false
    }
    fn is_pending(&self, _: &str) -> bool {
        false
    }
}

/// No-op council repository — all operations return empty/Ok.
pub struct NoopOrchestratorRepo;

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

// ─── McpServerRepository mock (includes update_last_connected) ────────────

/// Empty MCP repository — list returns empty, lookups return NotFound.
pub struct EmptyMcpRepo;

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

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Build a `CouncilDeps` with all no-op implementations.
pub fn make_orchestrator_deps() -> CouncilDeps {
    CouncilDeps {
        runner: Arc::new(NoopRunner),
        approval_registry: Arc::new(NoopApprovalRegistry),
        council_repo: Arc::new(NoopOrchestratorRepo),
    }
}

/// Build an `McpService` backed by an empty repository and no-op emitter.
pub fn make_mcp_service() -> Arc<McpService> {
    Arc::new(McpService::new(
        Arc::new(EmptyMcpRepo),
        Arc::new(NoopEmitter::new()),
    ))
}
