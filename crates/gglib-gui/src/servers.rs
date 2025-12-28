//! Server lifecycle operations for GUI backend.

use std::collections::HashMap;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use gglib_core::domain::Model;
use gglib_core::events::{AppEvent, ServerSummary};
use gglib_core::ports::{ProcessHandle, ServerConfig, ServerHealthStatus};

use crate::deps::GuiDeps;
use crate::error::GuiError;
use crate::types::{ServerInfo, StartServerRequest, StartServerResponse};

/// Handle for a running health monitor task.
struct MonitorHandle {
    join_handle: JoinHandle<()>,
    cancel_token: CancellationToken,
    model_id: i64,
}

/// Registry for tracking active server health monitors.
///
/// Manages lifecycle of monitoring tasks with unique server IDs.
struct ServerMonitorRegistry {
    monitors: HashMap<i64, MonitorHandle>,
    next_server_id: AtomicI64,
}

impl ServerMonitorRegistry {
    fn new() -> Self {
        Self {
            monitors: HashMap::new(),
            next_server_id: AtomicI64::new(1),
        }
    }

    /// Generate a unique server instance ID.
    fn generate_server_id(&self) -> i64 {
        self.next_server_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Add a monitor to the registry.
    fn add(
        &mut self,
        server_id: i64,
        join_handle: JoinHandle<()>,
        cancel_token: CancellationToken,
        port: u16,
        model_id: i64,
    ) {
        self.monitors.insert(
            server_id,
            MonitorHandle {
                join_handle,
                cancel_token,
                model_id,
            },
        );
        debug!(
            server_id,
            model_id, port, "Added server monitor to registry"
        );
    }

    /// Find monitor by model_id (for stop operations).
    fn find_by_model_id(&self, model_id: i64) -> Option<i64> {
        self.monitors
            .iter()
            .find(|(_, handle)| handle.model_id == model_id)
            .map(|(server_id, _)| *server_id)
    }

    /// Cancel and remove a monitor from the registry.
    async fn cancel(&mut self, server_id: i64) -> Result<(), GuiError> {
        if let Some(handle) = self.monitors.remove(&server_id) {
            debug!(server_id, "Cancelling server monitor");
            handle.cancel_token.cancel();

            // Wait for monitor task to finish (with timeout)
            match tokio::time::timeout(std::time::Duration::from_secs(2), handle.join_handle).await
            {
                Ok(Ok(())) => {
                    debug!(server_id, "Monitor task completed");
                    Ok(())
                }
                Ok(Err(e)) => {
                    warn!(server_id, error = %e, "Monitor task panicked");
                    Err(GuiError::Internal(format!("Monitor task panicked: {}", e)))
                }
                Err(_) => {
                    warn!(server_id, "Monitor task cancellation timed out");
                    Ok(()) // Continue anyway, task will be dropped
                }
            }
        } else {
            Ok(()) // Already removed or never existed
        }
    }
}

impl Drop for ServerMonitorRegistry {
    fn drop(&mut self) {
        // Cancel all monitors on drop
        for (server_id, handle) in self.monitors.drain() {
            debug!(server_id, "Cancelling monitor during registry drop");
            handle.cancel_token.cancel();
        }
    }
}

/// Server operations handler.
pub struct ServerOps<'a> {
    deps: &'a GuiDeps,
    monitors: Arc<Mutex<ServerMonitorRegistry>>,
}

impl<'a> ServerOps<'a> {
    pub fn new(deps: &'a GuiDeps) -> Self {
        Self {
            deps,
            monitors: Arc::new(Mutex::new(ServerMonitorRegistry::new())),
        }
    }

    /// Resolve model by ID, returning error if not found.
    async fn resolve_model(&self, id: i64) -> Result<Model, GuiError> {
        self.deps
            .models()
            .get_by_id(id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to query model: {e}")))?
            .ok_or_else(|| GuiError::NotFound {
                entity: "model",
                id: id.to_string(),
            })
    }

    /// Find a running process handle for a model.
    async fn find_handle(&self, model_id: i64) -> Option<ProcessHandle> {
        match self.deps.runner.list_running().await {
            Ok(handles) => handles.into_iter().find(|h| h.model_id == model_id),
            Err(_) => None,
        }
    }

    /// Build a ServerConfig from a model and GUI request.
    fn build_config(model: &Model, request: &StartServerRequest, base_port: u16) -> ServerConfig {
        let mut config = ServerConfig::new(
            model.id,
            model.name.clone(),
            model.file_path.clone(),
            base_port,
        );

        if let Some(ctx) = request.context_length.or(model.context_length) {
            config = config.with_context_size(ctx);
        }

        if let Some(port) = request.port {
            config = config.with_port(port);
        }

        let mut extra_args = Vec::new();

        if let Some(true) = request.jinja {
            extra_args.push("--jinja".to_string());
        }

        if let Some(ref format) = request.reasoning_format
            && format != "none"
        {
            extra_args.push("--reasoning-format".to_string());
            extra_args.push(format.clone());
        }

        if !extra_args.is_empty() {
            config.extra_args = extra_args;
        }

        config
    }

    /// Start serving a model.
    pub async fn start(
        &self,
        id: i64,
        request: StartServerRequest,
    ) -> Result<StartServerResponse, GuiError> {
        debug!(model_id = %id, "Starting server for model");

        if let Some(handle) = self.find_handle(id).await {
            return Ok(StartServerResponse {
                port: handle.port,
                message: format!("Server already running on port {}", handle.port),
            });
        }

        let model = self.resolve_model(id).await?;

        if !model.file_path.exists() {
            return Err(GuiError::ValidationFailed(format!(
                "Model file not found: {}",
                model.file_path.display()
            )));
        }

        // Resolve base_port from settings at serve-time (not bootstrap-time)
        use crate::proxy::resolve_llama_base_port;
        let settings = self
            .deps
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to load settings: {}", e)))?;
        let (base_port, source) = resolve_llama_base_port(None, &settings)?;
        debug!(
            base_port = %base_port,
            source = %source,
            "Resolved llama-server base port for model serving"
        );

        let config = Self::build_config(&model, &request, base_port);
        let handle = self.deps.runner.start(config).await.map_err(|e| {
            // Emit error event before mapping the error
            let error_summary = ServerSummary {
                id: format!("server-{}", id),
                model_id: id.to_string(),
                model_name: model.name.clone(),
                port: 0, // No port on failure
                healthy: Some(false),
            };
            self.deps
                .server_events()
                .error(&error_summary, &e.to_string());

            // Map ProcessError to semantic GuiError
            // Check if the error message indicates llama-server binary issues
            let err_str = e.to_string();

            if err_str.contains("llama-server binary not found")
                || err_str.contains("Failed to spawn llama-server")
                || err_str.contains("No such file or directory")
            {
                // Parse error details for structured response
                let expected_path = if let Some(start) = err_str.find("at: ") {
                    let path_start = start + 4;
                    if let Some(end) = err_str[path_start..].find('\n') {
                        err_str[path_start..path_start + end].to_string()
                    } else {
                        "~/.local/share/gglib/.llama/bin/llama-server".to_string()
                    }
                } else {
                    "~/.local/share/gglib/.llama/bin/llama-server".to_string()
                };

                let legacy_path = if err_str.contains("Found an older installation at:") {
                    err_str
                        .lines()
                        .find(|l| l.contains("Found an older installation at:"))
                        .and_then(|l| l.split("at: ").nth(1))
                        .map(|s| s.trim().to_string())
                } else {
                    None
                };

                let reason = if err_str.contains("not executable") {
                    "not executable".to_string()
                } else if err_str.contains("Permission denied") {
                    "permission denied".to_string()
                } else {
                    "not found".to_string()
                };

                GuiError::LlamaServerNotInstalled {
                    expected_path,
                    legacy_path,
                    suggested_command: "gglib llama install".to_string(),
                    reason,
                }
            } else {
                GuiError::Internal(format!("Failed to start server: {e}"))
            }
        })?;

        debug!(model_id = %id, port = %handle.port, "Server started successfully");

        // Emit server:started event
        let summary = ServerSummary {
            id: format!("server-{}", id),
            model_id: id.to_string(),
            model_name: model.name.clone(),
            port: handle.port,
            healthy: Some(true), // Assume healthy on successful start
        };
        self.deps.server_events().started(&summary);

        // Spawn health monitor after successful start
        self.spawn_health_monitor(handle.clone(), id).await;

        Ok(StartServerResponse {
            port: handle.port,
            message: format!("Server started on port {}", handle.port),
        })
    }

    /// Spawn a health monitoring task for a server.
    async fn spawn_health_monitor(&self, handle: ProcessHandle, model_id: i64) {
        let server_id = {
            let registry = self.monitors.lock().await;
            registry.generate_server_id()
        };

        let cancel_token = CancellationToken::new();
        let emitter = Arc::clone(self.deps.emitter());
        let port = handle.port;

        // Create monitor with 10-second check interval
        let monitor = gglib_runtime::ServerHealthMonitor::new(
            handle,
            std::time::Duration::from_secs(10),
            cancel_token.clone(),
        );

        // Spawn monitoring task
        let join_handle = tokio::spawn(async move {
            let stream = monitor.monitor();
            let mut stream = pin!(stream);

            while let Some(status) = stream.next().await {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                let detail = match &status {
                    ServerHealthStatus::Degraded { reason } => Some(reason.clone()),
                    ServerHealthStatus::Unreachable { last_error } => Some(last_error.clone()),
                    _ => None,
                };

                debug!(
                    server_id,
                    model_id,
                    port,
                    ?status,
                    "Health status changed, emitting event"
                );

                emitter.emit(AppEvent::ServerHealthChanged {
                    server_id,
                    model_id,
                    status,
                    detail,
                    timestamp,
                });
            }

            debug!(server_id, model_id, "Health monitor task completed");
        });

        // Register the monitor
        let mut registry = self.monitors.lock().await;
        registry.add(server_id, join_handle, cancel_token, port, model_id);
    }

    /// Stop serving a model.
    pub async fn stop(&self, id: i64) -> Result<String, GuiError> {
        debug!(model_id = %id, "Stopping server");

        let handle = self
            .find_handle(id)
            .await
            .ok_or_else(|| GuiError::NotFound {
                entity: "server",
                id: id.to_string(),
            })?;

        // Get model info for event emission
        let model = self.resolve_model(id).await?;

        // Emit stopping event
        let summary = ServerSummary {
            id: format!("server-{}", id),
            model_id: id.to_string(),
            model_name: model.name.clone(),
            port: handle.port,
            healthy: None, // Unknown during shutdown
        };
        self.deps.server_events().stopping(&summary);

        // Cancel monitoring first
        let server_id = {
            let registry = self.monitors.lock().await;
            registry.find_by_model_id(id)
        };

        if let Some(server_id) = server_id {
            let mut registry = self.monitors.lock().await;
            registry.cancel(server_id).await?;
        }

        // Then stop the server process
        self.deps.runner.stop(&handle).await.map_err(|e| {
            // Emit error event on stop failure
            self.deps.server_events().error(&summary, &e.to_string());
            GuiError::Internal(format!("Failed to stop server: {e}"))
        })?;

        // Emit stopped event after successful stop
        self.deps.server_events().stopped(&summary);

        Ok(format!("Server for model {} stopped", id))
    }

    /// Stop all running servers.
    ///
    /// Used during application shutdown to ensure all llama-server processes are terminated.
    pub async fn stop_all(&self) -> Result<(), GuiError> {
        debug!("Stopping all servers");

        // Get all running servers
        let handles =
            self.deps.runner.list_running().await.map_err(|e| {
                GuiError::Internal(format!("Failed to list running servers: {}", e))
            })?;

        let model_ids: Vec<i64> = handles.iter().map(|h| h.model_id).collect();

        debug!("Found {} running servers to stop", model_ids.len());

        // Stop each server
        for model_id in model_ids {
            if let Err(e) = self.stop(model_id).await {
                warn!("Failed to stop server {}: {}", model_id, e);
                // Continue stopping others even if one fails
            }
        }

        Ok(())
    }

    /// List all running servers as GUI DTOs.
    pub async fn list_servers(&self) -> Vec<ServerInfo> {
        match self.deps.runner.list_running().await {
            Ok(handles) => handles.iter().map(ServerInfo::from_handle).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Get logs for a specific server port.
    pub fn get_logs(&self, port: u16) -> Vec<crate::types::ServerLogEntry> {
        gglib_runtime::get_log_manager().get_logs(port)
    }

    /// Subscribe to real-time log events.
    /// Returns a broadcast receiver for ServerLogEntry events.
    pub fn subscribe_logs(&self) -> tokio::sync::broadcast::Receiver<crate::types::ServerLogEntry> {
        gglib_runtime::get_log_manager().subscribe()
    }

    /// Clear logs for a specific server port.
    pub fn clear_logs(&self, port: u16) {
        gglib_runtime::get_log_manager().clear_logs(port);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};

    /// Helper to check if registry contains a server_id
    impl ServerMonitorRegistry {
        #[cfg(test)]
        fn contains(&self, server_id: i64) -> bool {
            self.monitors.contains_key(&server_id)
        }
    }

    #[tokio::test]
    async fn registry_add_cancel_removes_entry() {
        let mut reg = ServerMonitorRegistry::new();

        let token = CancellationToken::new();
        let task_token = token.clone();
        let handle = tokio::spawn(async move {
            task_token.cancelled().await;
        });

        let server_id = reg.generate_server_id();
        reg.add(server_id, handle, token, 8080, 1);

        assert!(reg.contains(server_id));

        // Cancel should remove entry
        let result = reg.cancel(server_id).await;
        assert!(result.is_ok());
        assert!(!reg.contains(server_id));
    }

    #[tokio::test]
    async fn registry_cancel_is_idempotent() {
        let mut reg = ServerMonitorRegistry::new();

        // Cancel non-existent entry should not panic
        let result = reg.cancel(999).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn registry_cancel_completes_quickly() {
        let mut reg = ServerMonitorRegistry::new();

        let token = CancellationToken::new();
        let task_token = token.clone();
        let handle = tokio::spawn(async move {
            task_token.cancelled().await;
        });

        let server_id = reg.generate_server_id();
        reg.add(server_id, handle, token, 8080, 1);

        // Cancel should complete within timeout
        let res = timeout(Duration::from_secs(3), reg.cancel(server_id)).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn registry_find_by_model_id_works() {
        let mut reg = ServerMonitorRegistry::new();

        let token = CancellationToken::new();
        let task_token = token.clone();
        let handle = tokio::spawn(async move {
            task_token.cancelled().await;
        });

        let server_id = reg.generate_server_id();
        let model_id = 42;
        reg.add(server_id, handle, token, 8080, model_id);

        // Should find by model_id
        assert_eq!(reg.find_by_model_id(model_id), Some(server_id));

        // Should not find unknown model_id
        assert_eq!(reg.find_by_model_id(999), None);

        // Cleanup
        let _ = reg.cancel(server_id).await;
    }

    #[tokio::test]
    async fn registry_generate_server_id_is_unique() {
        let reg = ServerMonitorRegistry::new();

        let id1 = reg.generate_server_id();
        let id2 = reg.generate_server_id();
        let id3 = reg.generate_server_id();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[tokio::test]
    async fn registry_drop_cancels_all_monitors() {
        // This test verifies that Drop implementation cancels tokens
        let token1 = CancellationToken::new();
        let token2 = CancellationToken::new();

        let check_token1 = token1.clone();
        let check_token2 = token2.clone();

        {
            let mut reg = ServerMonitorRegistry::new();

            let task_token1 = token1.clone();
            let handle1 = tokio::spawn(async move {
                task_token1.cancelled().await;
            });

            let task_token2 = token2.clone();
            let handle2 = tokio::spawn(async move {
                task_token2.cancelled().await;
            });

            reg.add(1, handle1, token1, 8080, 1);
            reg.add(2, handle2, token2, 8081, 2);

            // reg goes out of scope here, triggering Drop
        }

        // After drop, tokens should be cancelled
        assert!(check_token1.is_cancelled());
        assert!(check_token2.is_cancelled());
    }

    // =========================================================================
    // ServerEvents recording tests
    // =========================================================================

    use gglib_core::events::{ServerEvents, ServerSummary};
    use std::sync::Mutex;

    /// Recording implementation of ServerEvents for testing.
    ///
    /// Records all event calls in a vector for later assertion.
    #[derive(Default)]
    struct RecordingServerEvents {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingServerEvents {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl ServerEvents for RecordingServerEvents {
        fn started(&self, server: &ServerSummary) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("started:{}", server.model_name));
        }

        fn stopping(&self, server: &ServerSummary) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("stopping:{}", server.model_name));
        }

        fn stopped(&self, server: &ServerSummary) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("stopped:{}", server.model_name));
        }

        fn snapshot(&self, servers: &[ServerSummary]) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("snapshot:{}", servers.len()));
        }

        fn error(&self, server: &ServerSummary, error: &str) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("error:{}:{}", server.model_name, error));
        }
    }

    #[tokio::test]
    async fn test_server_events_recording() {
        // This test demonstrates that ServerEvents trait can be used
        // for testing without requiring real SSE/Tauri infrastructure
        let recorder = RecordingServerEvents::new();

        let summary = ServerSummary {
            id: "test-server-1".to_string(),
            model_id: "42".to_string(),
            model_name: "TestModel".to_string(),
            port: 8080,
            healthy: Some(true),
        };

        recorder.started(&summary);
        recorder.stopping(&summary);
        recorder.stopped(&summary);
        recorder.error(&summary, "test error");

        let calls = recorder.get_calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0], "started:TestModel");
        assert_eq!(calls[1], "stopping:TestModel");
        assert_eq!(calls[2], "stopped:TestModel");
        assert_eq!(calls[3], "error:TestModel:test error");
    }
}
