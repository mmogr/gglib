//! Server lifecycle operations for GUI backend.

use std::sync::Arc;

use tracing::{debug, warn};

use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, ModelRuntimeError};
use gglib_core::services::AppCore;

use crate::error::GuiError;
use crate::launch_options::plan_bare_launch;
use crate::proxy::ProxyOps;
use crate::types::{ServerInfo, StartServerRequest, StartServerResponse};

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
    /// Where a start or stop the runtime refuses is announced. The load and
    /// the unload themselves are announced by the runtime's observer
    /// (`ResidencyEvents`), whoever caused them, so nothing here says
    /// `server_started` or `server_stopped`.
    pub emitter: Arc<dyn AppEventEmitter>,
}

/// Server operations handler.
pub struct ServerOps {
    deps: ServerDeps,
}

impl ServerOps {
    pub fn new(deps: ServerDeps) -> Self {
        Self { deps }
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
        // A launch announces itself once healthy; a model already resident
        // is served as it stands and announces nothing.
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

        // The llama-server port, not the proxy's: existing GUI flows talk to
        // the model directly, and the proxy runs alongside for observability.
        Ok(StartServerResponse {
            port: target.port,
            message: format!("Server started on port {}", target.port),
        })
    }

    /// Stop serving a model, in whichever slot it sits: the primary, or the
    /// second, where an image model beside a chat model usually is.
    pub async fn stop(&self, id: i64) -> Result<String, GuiError> {
        debug!(model_id = %id, "Stopping server");

        let not_running = || GuiError::NotFound {
            entity: "server",
            id: id.to_string(),
        };
        let model_id = u32::try_from(id).map_err(|_| not_running())?;
        let running = self.deps.proxy.runtime().list_running().await;
        if !running.iter().any(|h| h.model_id == id) {
            return Err(not_running());
        }

        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;

        // The runtime announces the unload, before it stops the process.
        let stopped = self
            .deps
            .proxy
            .runtime()
            .stop_model(model_id)
            .await
            .map_err(|e| {
                self.deps
                    .emitter
                    .emit(AppEvent::server_error(Some(id), &model.name, (&e).into()));
                GuiError::Internal(format!("Failed to stop server: {e}"))
            })?;
        // Listed a moment ago, gone now: it stopped on its own, or another
        // caller stopped it first. Either way it is not running.
        if !stopped {
            return Err(not_running());
        }

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
        // Not the server failing: nothing to draw with yet. The runtime's
        // words name the remedy.
        ModelRuntimeError::ImageRuntimeNotInstalled => GuiError::Unavailable(err.to_string()),
        _ => GuiError::Internal(format!("Failed to start server: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // DB-backed ServerOps tests
    // =========================================================================

    use gglib_core::ports::NoopEmitter;

    use crate::test_support::test_core_and_proxy;

    async fn make_server_ops() -> ServerOps {
        let (core, proxy) = test_core_and_proxy().await;
        ServerOps::new(ServerDeps {
            core,
            proxy,
            emitter: Arc::new(NoopEmitter::new()),
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
    }
}

#[cfg(test)]
#[path = "servers_events_tests.rs"]
mod events_tests;
