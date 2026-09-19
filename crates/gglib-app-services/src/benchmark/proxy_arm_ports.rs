//! The three stand-ins the proxy arm's proxy is handed in place of the
//! daemon's own: a runtime port pinned to the eval's held model, fixed
//! settings, and an MCP repository with no servers. The module docs of
//! `proxy_arm.rs` say why each is what it is.

use async_trait::async_trait;
use gglib_core::domain::mcp::{McpServer, NewMcpServer};
use gglib_core::ports::{
    Admission, LaunchOverrides, McpRepositoryError, McpServerRepository, ModelRuntimeError,
    ModelRuntimePort, RepositoryError, RunningTarget, SettingsRepository,
};
use gglib_core::{LoopGuardMode, Settings};

/// A runtime port that serves one already-admitted target and nothing else.
#[derive(Debug)]
pub(super) struct PinnedTarget {
    pub(super) target: RunningTarget,
}

#[async_trait]
impl ModelRuntimePort for PinnedTarget {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        if model_name != self.target.model_name {
            return Err(ModelRuntimeError::PinnedModelMismatch {
                expected: self.target.model_name.clone(),
                requested: model_name.to_owned(),
            });
        }
        // Detached: the eval's own lease holds the model for every arm.
        Ok(Admission::detached(self.target.clone()))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    // Refused, never done. Once the proxy's watchdog has asked for a
    // recycle, each later request that finds the upstream idle asks again,
    // logs the refusal and re-arms, so a degraded model during the arm shows
    // as repeated warnings in the log rather than as a relaunch.
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Err(ModelRuntimeError::Internal(
            "the agentic eval owns this model's lifecycle".to_owned(),
        ))
    }

    fn pinned_model(&self) -> Option<String> {
        Some(self.target.model_name.clone())
    }
}

/// The settings the proxy arm runs under, whatever this machine's are.
#[derive(Debug)]
pub(super) struct FixedSettings(Settings);

impl FixedSettings {
    pub(super) fn for_eval() -> Self {
        Self(Settings {
            trust_client_sampling: Some(true),
            tool_call_repair: Some(true),
            loop_guard_mode: Some(LoopGuardMode::Note),
            ..Settings::default()
        })
    }

    /// The loop-guard mode the proxy reads from these settings, which the
    /// report records.
    pub(super) const fn loop_guard_mode(&self) -> LoopGuardMode {
        self.0.effective_loop_guard_mode()
    }
}

#[async_trait]
impl SettingsRepository for FixedSettings {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(self.0.clone())
    }

    async fn save(&self, _settings: &Settings) -> Result<(), RepositoryError> {
        Err(RepositoryError::Storage(
            "the proxy arm's settings are fixed by the eval".to_owned(),
        ))
    }
}

/// An MCP repository with no servers in it.
pub(super) struct NoMcpServers;

#[async_trait]
impl McpServerRepository for NoMcpServers {
    async fn insert(&self, _server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal(
            "the proxy arm has no MCP servers".into(),
        ))
    }

    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }

    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.to_owned()))
    }

    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(Vec::new())
    }

    async fn update(&self, _server: &McpServer) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::Internal(
            "the proxy arm has no MCP servers".into(),
        ))
    }

    async fn delete(&self, id: i64) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }

    async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
}
