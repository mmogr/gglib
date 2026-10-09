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

use gglib_core::events::AppEvent;
use gglib_core::ports::{
    AppEventEmitter, ModelRuntimeError, ProcessHandle, ServerHealthStatus, ToolSupportDetectorPort,
};
use gglib_core::services::AppCore;

use crate::error::GuiError;
use crate::launch_options::plan_bare_launch;
use crate::proxy::ProxyOps;
use crate::types::{ServerInfo, StartServerRequest, StartServerResponse, ToolSupportResponse};

/// Dependencies for server lifecycle operations.
pub struct ServerDeps {
    pub core: Arc<AppCore>,
    /// The proxy the GUI drives models through.
    ///
    /// Starting a model from the GUI goes through the same pipeline as
    /// `gglib proxy` and `gglib serve`, so it has the dashboard, cache
    /// lifecycle and request normalization those have, and cannot contend
    /// with the proxy for the GPU by running a second llama-server alongside
    /// it.
    pub proxy: Arc<ProxyOps>,
    /// Where every server event goes: started, stopped, error and health.
    pub emitter: Arc<dyn AppEventEmitter>,
    pub tool_detector: Arc<dyn ToolSupportDetectorPort>,
}

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

    /// Find monitor by `model_id` (for stop operations).
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
                    Err(GuiError::Internal(format!("Monitor task panicked: {e}")))
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
pub struct ServerOps {
    deps: ServerDeps,
    monitors: Arc<Mutex<ServerMonitorRegistry>>,
}

impl ServerOps {
    pub fn new(deps: ServerDeps) -> Self {
        Self {
            deps,
            monitors: Arc::new(Mutex::new(ServerMonitorRegistry::new())),
        }
    }

    /// Start serving a model.
    pub async fn start(
        &self,
        id: i64,
        request: StartServerRequest,
    ) -> Result<StartServerResponse, GuiError> {
        debug!(model_id = %id, "Starting server for model");

        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;

        if !model.file_path.exists() {
            return Err(GuiError::ValidationFailed(format!(
                "Model file not found: {}",
                model.file_path.display()
            )));
        }

        let settings = self.deps.core.settings().get().await?;

        // The proxy must be up before the model: it owns the runtime the model
        // will run under, and its dashboard and cache lifecycle are the reason
        // this path exists.
        let proxy_addr = self.deps.proxy.ensure_running().await?;
        debug!(%proxy_addr, "proxy ready for model start");

        // The cascade `gglib serve` and a pinned start run, less their proxy
        // inputs, so a request means the same launch options here as there.
        let (default_ctx, overrides) = plan_bare_launch(&model, &settings, &request);

        // The lease is dropped as soon as the model is up: this is a "start
        // this model" request, not a request being served. The model stays
        // resident until admission control gives its slot to something else.
        let target = self
            .deps
            .proxy
            .runtime()
            .admit(&model.name, request.context_length, default_ctx, overrides)
            .await
            .map(gglib_core::ports::Admission::into_target)
            .map_err(|e| {
                self.deps
                    .emitter
                    .emit(AppEvent::server_error(Some(id), &model.name, (&e).into()));
                map_runtime_error(&e)
            })?;

        debug!(model_id = %id, port = %target.port, "Server started successfully");

        self.deps
            .emitter
            .emit(AppEvent::server_started(id, &model.name, target.port));

        let handle = ProcessHandle::new(id, model.name.clone(), None, target.port, now_secs());
        self.spawn_health_monitor(handle, id).await;

        // The llama-server port, not the proxy's: existing GUI flows talk to
        // the model directly, and the proxy runs alongside for observability.
        Ok(StartServerResponse {
            port: target.port,
            message: format!("Server started on port {}", target.port),
        })
    }

    /// Spawn a health monitoring task for a server.
    async fn spawn_health_monitor(&self, handle: ProcessHandle, model_id: i64) {
        let server_id = {
            let registry = self.monitors.lock().await;
            registry.generate_server_id()
        };

        let cancel_token = CancellationToken::new();
        let emitter = Arc::clone(&self.deps.emitter);
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

        let running = self.deps.proxy.runtime().current_model().await;
        if running.is_none_or(|t| i64::from(t.model_id) != id) {
            return Err(GuiError::NotFound {
                entity: "server",
                id: id.to_string(),
            });
        }

        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;

        // Cancel monitoring first, so a shutdown is not reported as a health
        // regression.
        let server_id = {
            let registry = self.monitors.lock().await;
            registry.find_by_model_id(id)
        };
        if let Some(server_id) = server_id {
            let mut registry = self.monitors.lock().await;
            registry.cancel(server_id).await?;
        }

        self.deps
            .proxy
            .runtime()
            .stop_current()
            .await
            .map_err(|e| {
                self.deps
                    .emitter
                    .emit(AppEvent::server_error(Some(id), &model.name, (&e).into()));
                GuiError::Internal(format!("Failed to stop server: {e}"))
            })?;

        self.deps
            .emitter
            .emit(AppEvent::server_stopped(id, &model.name));

        Ok(format!("Server for model {id} stopped"))
    }

    /// Stop all running servers.
    ///
    /// Used during application shutdown to ensure all llama-server processes
    /// are terminated.
    pub async fn stop_all(&self) -> Result<(), GuiError> {
        debug!("Stopping all servers");

        let model_ids: Vec<i64> = self
            .deps
            .proxy
            .runtime()
            .list_running()
            .await
            .iter()
            .map(|h| h.model_id)
            .collect();

        debug!("Found {} running servers to stop", model_ids.len());

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
        self.deps
            .proxy
            .runtime()
            .list_running()
            .await
            .iter()
            .map(ServerInfo::from_handle)
            .collect()
    }

    /// Get logs for a specific server port.
    pub fn get_logs(&self, port: u16) -> Vec<crate::types::ServerLogEntry> {
        gglib_runtime::get_log_manager().get_logs(port)
    }

    /// Subscribe to real-time log events.
    /// Returns a broadcast receiver for `ServerLogEntry` events.
    pub fn subscribe_logs(&self) -> tokio::sync::broadcast::Receiver<crate::types::ServerLogEntry> {
        gglib_runtime::get_log_manager().subscribe()
    }

    /// Get tool support detection for a running server's model.
    ///
    /// Sources `supports_tool_calls` from the model's `ModelCapabilities` bitflags
    /// stored in the database.
    /// `confidence` and `detected_format` are derived by running the detector with
    /// the chat template already stored in `model.metadata` — no disk I/O required.
    pub async fn get_server_tool_support(
        &self,
        model_id: i64,
    ) -> Result<ToolSupportResponse, GuiError> {
        use gglib_core::domain::ModelCapabilities;
        use gglib_core::ports::{ModelSource, ToolSupportDetectionInput};

        let model = crate::helpers::resolve_model(self.deps.core.models(), model_id).await?;

        // Primary boolean comes from the authoritative DB capabilities bitflag.
        let supports_tool_calls = model
            .capabilities
            .contains(ModelCapabilities::SUPPORTS_TOOL_CALLS);

        // Chat template is already in model.metadata (loaded by the same DB query).
        // Passing it to the detector ensures accurate format/confidence values even
        // for custom-named models where filename heuristics would otherwise fail.
        let chat_template = model
            .metadata
            .get("tokenizer.chat_template")
            .map(String::as_str);

        let detection = self.deps.tool_detector.detect(ToolSupportDetectionInput {
            model_id: model.file_path.to_str().unwrap_or(""),
            chat_template,
            tags: &[],
            source: ModelSource::LocalGguf,
        });

        Ok(ToolSupportResponse {
            supports_tool_calls,
            confidence: detection.confidence,
            detected_format: detection.detected_format.map(|f| f.to_string()),
        })
    }
}

/// Seconds since the Unix epoch, for process-handle timestamps.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Classify a spawn failure as a llama-server availability problem.
///
/// Returns the `reason` for [`GuiError::LlamaServerNotInstalled`], or `None`
/// when the spawn failed for a cause the user cannot fix by installing — an OOM
/// kill, a bound port, a missing model file.
///
/// Inspecting text is unavoidable here: `build_and_spawn` renders
/// `LlamaServerError` into `anyhow` before `SpawnFailed` wraps it, so the typed
/// variant is long gone by the time it reaches this layer. Restricting the
/// probe to the `SpawnFailed` arm is what keeps it honest — no other failure
/// mode can trip it by coincidence.
fn llama_server_unavailable_reason(msg: &str) -> Option<&'static str> {
    if !(msg.contains("llama-server binary")
        || msg.contains("Failed to spawn llama-server")
        || msg.contains("No such file or directory"))
    {
        return None;
    }

    Some(if msg.contains("not executable") {
        "not executable"
    } else if msg.contains("Permission denied") {
        "permission denied"
    } else {
        "not found"
    })
}

/// Map a runtime failure onto the GUI error the frontend expects.
///
/// Preserves the llama-server-not-installed hint, which is the one failure a
/// user can act on directly. Only [`ModelRuntimeError::SpawnFailed`] is probed
/// for it — that is the sole variant a binary problem can arrive as, so no
/// other failure gets to claim the install prompt by wording alone.
fn map_runtime_error(err: &ModelRuntimeError) -> GuiError {
    match err {
        ModelRuntimeError::ModelNotFound(name) => GuiError::NotFound {
            entity: "model",
            id: name.clone(),
        },
        ModelRuntimeError::SpawnFailed(msg) => llama_server_unavailable_reason(msg).map_or_else(
            || GuiError::Internal(format!("Failed to start server: {err}")),
            GuiError::llama_server_not_installed,
        ),
        // Serving an image model is the caller's mistake, said as the runtime
        // says it, and not a failure of the server.
        ModelRuntimeError::ImageModelCannotChat(_)
        | ModelRuntimeError::ImageModelIncomplete { .. } => {
            GuiError::ValidationFailed(err.to_string())
        }
        // Not the server failing: nothing to draw with yet, or no room beside
        // a held model. The runtime's words name the remedy.
        ModelRuntimeError::ImageRuntimeNotInstalled
        | ModelRuntimeError::ImageModelDoesNotFit { .. } => GuiError::Unavailable(err.to_string()),
        _ => GuiError::Internal(format!("Failed to start server: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};

    /// Helper to check if registry contains a `server_id`
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
    // DB-backed ServerOps tests
    // =========================================================================

    use gglib_core::ports::NoopEmitter;

    use crate::test_support::{MockToolSupportDetector, test_core_and_proxy};

    async fn make_server_ops() -> ServerOps {
        let (core, proxy) = test_core_and_proxy().await;
        ServerOps::new(ServerDeps {
            core,
            proxy,
            emitter: Arc::new(NoopEmitter::new()),
            tool_detector: Arc::new(MockToolSupportDetector),
        })
    }

    #[tokio::test]
    async fn list_servers_empty_on_fresh_db() {
        let ops = make_server_ops().await;
        assert!(ops.list_servers().await.is_empty());
    }

    /// With nothing running, the proxy runtime reports no current model, so a
    /// stop must surface as `NotFound` rather than a generic failure.
    #[tokio::test]
    async fn stop_nonexistent_model_returns_not_found() {
        let ops = make_server_ops().await;
        let err = ops.stop(9999).await.expect_err("expected NotFound");
        assert!(
            matches!(err, GuiError::NotFound { .. }),
            "expected NotFound, got {err:?}"
        );
    }

    /// Nothing is running, so stopping everything is a no-op rather than an
    /// error — application shutdown depends on this.
    #[tokio::test]
    async fn stop_all_succeeds_with_no_servers() {
        let ops = make_server_ops().await;
        assert!(ops.stop_all().await.is_ok());
    }

    // ---------------------------------------------------------------
    // Runtime error mapping
    // ---------------------------------------------------------------

    /// The `reason` of a [`GuiError::LlamaServerNotInstalled`], or a panic
    /// naming what came back instead.
    fn install_hint_reason(err: &GuiError) -> &str {
        match err {
            GuiError::LlamaServerNotInstalled { reason, .. } => reason,
            other => panic!("expected LlamaServerNotInstalled, got {other:?}"),
        }
    }

    /// A missing binary is the one start failure a user can fix themselves, so
    /// it has to reach the frontend as the actionable install prompt.
    #[test]
    fn missing_binary_maps_to_the_install_hint() {
        let err = map_runtime_error(&ModelRuntimeError::SpawnFailed(
            "llama-server binary not found at: /home/u/.local/share/gglib/.llama/bin/llama-server\n\nPlease install llama.cpp by running:\n  gglib config llama install".to_string(),
        ));

        assert_eq!(install_hint_reason(&err), "not found");
    }

    /// The OS-level spawn failure carries no llama-server wording of its own,
    /// only errno text — it still means the binary is not usable.
    #[test]
    fn failed_spawn_maps_to_the_install_hint() {
        let err = map_runtime_error(&ModelRuntimeError::SpawnFailed(
            "Failed to spawn llama-server: No such file or directory (os error 2)".to_string(),
        ));

        assert_eq!(install_hint_reason(&err), "not found");
    }

    /// A present-but-unusable binary is a distinct fix from a missing one, so
    /// the reason has to say so rather than collapsing to "not found".
    #[test]
    fn non_executable_binary_reports_its_own_reason() {
        let err = map_runtime_error(&ModelRuntimeError::SpawnFailed(
            "llama-server binary exists but is not executable: /home/u/.llama/bin/llama-server\n\nPlease check file permissions or reinstall with:\n  gglib config llama install".to_string(),
        ));

        assert_eq!(install_hint_reason(&err), "not executable");
    }

    /// Same again for the permission case — reinstalling will not help, so it
    /// must not be described as a missing binary.
    #[test]
    fn permission_denied_reports_its_own_reason() {
        let err = map_runtime_error(&ModelRuntimeError::SpawnFailed(
            "Permission denied accessing llama-server binary: /home/u/.llama/bin/llama-server\n\nPlease check file permissions.".to_string(),
        ));

        assert_eq!(install_hint_reason(&err), "permission denied");
    }

    /// `SpawnFailed` covers far more than binary problems. An OOM kill or a
    /// bound port must not be dressed up as a missing install, which is what
    /// matching the variant alone would do.
    #[test]
    fn unrelated_spawn_failures_stay_internal() {
        for msg in [
            "OOM killed",
            "port already in use",
            "Failed to get child PID",
        ] {
            let err = map_runtime_error(&ModelRuntimeError::SpawnFailed(msg.to_string()));
            assert!(
                matches!(err, GuiError::Internal(_)),
                "{msg} should map to Internal, got {err:?}"
            );
        }
    }

    /// Only `SpawnFailed` is probed for binary wording. Another variant that
    /// happens to quote the same errno text is not an install problem, and the
    /// old text-first gate got this wrong.
    #[test]
    fn lookalike_text_on_other_variants_is_not_an_install_problem() {
        let err = map_runtime_error(&ModelRuntimeError::Internal(
            "No such file or directory".to_string(),
        ));

        assert!(
            matches!(err, GuiError::Internal(_)),
            "expected Internal, got {err:?}"
        );
    }

    #[test]
    fn model_not_found_maps_to_not_found() {
        let err = map_runtime_error(&ModelRuntimeError::ModelNotFound("qwen2.5".to_string()));

        match err {
            GuiError::NotFound { entity, id } => {
                assert_eq!(entity, "model");
                assert_eq!(id, "qwen2.5");
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    /// Serving an image model is refused as the caller's mistake, in the
    /// runtime's words, which the HTTP layer answers with a 400.
    #[test]
    fn an_image_model_maps_to_a_validation_failure() {
        let err = map_runtime_error(&ModelRuntimeError::ImageModelCannotChat("flux".to_owned()));

        match err {
            GuiError::ValidationFailed(message) => {
                assert!(message.contains("'flux'"), "{message}");
                assert!(message.contains("image model"), "{message}");
            }
            other => panic!("expected ValidationFailed, got {other:?}"),
        }
    }

    /// An image model missing a file is the caller's to fix; no runtime to
    /// draw with, or no room beside a held model, is unavailability, not a
    /// server failure. Each keeps the runtime's words.
    #[test]
    fn image_refusals_map_to_validation_and_unavailable() {
        let incomplete = map_runtime_error(&ModelRuntimeError::ImageModelIncomplete {
            model: "flux".to_owned(),
            missing: vec![gglib_core::domain::ComponentRole::Vae],
        });
        assert!(
            matches!(&incomplete, GuiError::ValidationFailed(m) if m.contains("VAE")),
            "{incomplete:?}"
        );

        let not_installed = map_runtime_error(&ModelRuntimeError::ImageRuntimeNotInstalled);
        assert!(
            matches!(&not_installed, GuiError::Unavailable(m) if m.contains("gglib config sd install")),
            "{not_installed:?}"
        );

        let no_room = map_runtime_error(&ModelRuntimeError::ImageModelDoesNotFit {
            model: "flux".to_owned(),
            held_model: "qwen".to_owned(),
            needed_bytes: Some(1),
            free_bytes: None,
        });
        assert!(
            matches!(&no_room, GuiError::Unavailable(m) if m.contains("'qwen'")),
            "{no_room:?}"
        );
    }
}

#[cfg(test)]
#[path = "servers_events_tests.rs"]
mod events_tests;
