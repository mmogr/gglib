//! Tests for [`super::ProxySupervisor`] — start, stop and restart against
//! mock ports, with a real listener bound on port 0.
//!
//! Beside the module rather than inside it. Declared as `mod tests` through
//! `#[path]`, so every test keeps the `supervisor::tests::` path it had when
//! it lived inline.

use super::*;
use async_trait::async_trait;
use gglib_core::domain::mcp::{McpServer, NewMcpServer};
use gglib_core::ports::{
    CatalogError, ModelLaunchSpec, ModelRuntimeError, ModelSummary, RunningTarget,
};
use gglib_core::ports::{McpRepositoryError, McpServerRepository};
use gglib_core::ports::{RepositoryError, SettingsRepository};

/// Empty MCP repository for testing — all reads return empty/not-found.
#[derive(Debug)]
struct EmptyMcpRepo;

#[async_trait]
impl McpServerRepository for EmptyMcpRepo {
    async fn insert(&self, _: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("not implemented".into()))
    }
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.to_string()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(vec![])
    }
    async fn update(&self, _: &McpServer) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::Internal("not implemented".into()))
    }
    async fn delete(&self, id: i64) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
    async fn update_last_connected(&self, id: i64) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
}

/// Mock runtime port for testing.
#[derive(Debug)]
struct MockRuntimePort;

#[async_trait]
impl ModelRuntimePort for MockRuntimePort {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: gglib_core::ports::LaunchOverrides,
    ) -> Result<gglib_core::ports::Admission, ModelRuntimeError> {
        Ok(gglib_core::ports::Admission::detached(
            RunningTarget::local(8080, 1, "test-model".to_string(), 4096, false),
        ))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Mock catalog port for testing.
#[derive(Debug)]
struct MockCatalogPort;

#[async_trait]
impl ModelCatalogPort for MockCatalogPort {
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

fn make_ports() -> (Arc<dyn ModelRuntimePort>, Arc<dyn ModelCatalogPort>) {
    (Arc::new(MockRuntimePort), Arc::new(MockCatalogPort))
}

fn make_mcp() -> Arc<McpService> {
    Arc::new(McpService::new(Arc::new(EmptyMcpRepo)))
}

struct MockSettingsRepo;

#[async_trait]
impl SettingsRepository for MockSettingsRepo {
    async fn load(&self) -> Result<gglib_core::Settings, RepositoryError> {
        Ok(gglib_core::Settings::with_defaults())
    }
    async fn save(&self, _settings: &gglib_core::Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

fn make_settings_repo() -> Arc<dyn SettingsRepository> {
    Arc::new(MockSettingsRepo)
}

#[tokio::test]
async fn test_supervisor_lifecycle() {
    let supervisor = ProxySupervisor::new();

    // Initially stopped
    assert_eq!(supervisor.status().await, ProxyStatus::Stopped);

    // Start on random port
    let config = ProxyConfig {
        host: "127.0.0.1".to_string(),
        port: 0, // Random port
        default_context: Some(4096),
        cache_enabled: false,
        slot_dir: None,
        ..ProxyConfig::default()
    };
    let (runtime, catalog) = make_ports();
    let mcp = make_mcp();
    let addr = supervisor
        .start(
            config.clone(),
            runtime.clone(),
            catalog.clone(),
            mcp,
            make_settings_repo(),
        )
        .await
        .unwrap();
    assert_ne!(addr.addr.port(), 0);

    // Should be running
    match supervisor.status().await {
        ProxyStatus::Running { address } => assert_eq!(address, addr.addr),
        other => panic!("Expected Running, got {other:?}"),
    }

    // Can't start again
    let (runtime2, catalog2) = make_ports();
    assert!(matches!(
        supervisor
            .start(config, runtime2, catalog2, make_mcp(), make_settings_repo())
            .await,
        Err(SupervisorError::AlreadyRunning(_))
    ));

    // Stop
    supervisor.stop().await.unwrap();

    // Should be stopped
    assert_eq!(supervisor.status().await, ProxyStatus::Stopped);

    // Can't stop again
    assert!(matches!(
        supervisor.stop().await,
        Err(SupervisorError::NotRunning)
    ));
}

#[tokio::test]
async fn test_restart_after_stop() {
    let supervisor = ProxySupervisor::new();

    let config = ProxyConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        default_context: Some(4096),
        cache_enabled: false,
        slot_dir: None,
        ..ProxyConfig::default()
    };

    // Start
    let (runtime, catalog) = make_ports();
    let addr1 = supervisor
        .start(
            config.clone(),
            runtime,
            catalog,
            make_mcp(),
            make_settings_repo(),
        )
        .await
        .unwrap();

    // Stop
    supervisor.stop().await.unwrap();

    // Start again (should work)
    let (runtime2, catalog2) = make_ports();
    let addr2 = supervisor
        .start(config, runtime2, catalog2, make_mcp(), make_settings_repo())
        .await
        .unwrap();

    // Different port (both were 0 -> random)
    // Note: Could technically get same port, but very unlikely
    assert_ne!(addr1.addr.port(), 0);
    assert_ne!(addr2.addr.port(), 0);

    // Cleanup
    supervisor.stop().await.unwrap();
}
