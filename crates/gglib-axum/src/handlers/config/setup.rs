//! Setup handlers - first-run wizard system status and provisioning.

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::StreamExt;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};

use crate::dto::system::VulkanStatusDto;
use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::paths::{llama_cpp_dir, llama_server_path};
use gglib_app_services::setup::SetupStatus;
use gglib_runtime::llama::{
    Acceleration, BuildEvent, detect_optimal_acceleration, run_llama_source_build, vulkan_status,
};

/// Get the full system setup status for the first-run wizard.
pub async fn status(State(state): State<AppState>) -> Result<Json<SetupStatus>, HttpError> {
    Ok(Json(state.setup.get_status().await?))
}

/// Get Vulkan build-readiness status.
pub async fn vulkan_status_handler() -> Json<VulkanStatusDto> {
    Json(vulkan_status().into())
}

/// Install llama.cpp pre-built binaries with SSE progress streaming.
///
/// Returns an SSE stream with events:
/// - `progress`: `{ "downloaded": <bytes>, "total": <bytes> }`
/// - `complete`: `{}`
/// - `error`: `{ "message": "<error>" }`
pub async fn install_llama(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let (tx, rx) = tokio::sync::mpsc::channel::<LlamaProgressEvent>(64);
    let setup = state.setup.clone();

    tokio::spawn(async move {
        let tx_progress = tx.clone();
        let callback: Box<dyn Fn(u64, u64) + Send + Sync> =
            Box::new(move |downloaded: u64, total: u64| {
                // Best-effort send; if the receiver dropped, ignore
                let _ = tx_progress.try_send(LlamaProgressEvent::Progress { downloaded, total });
            });

        match setup.install_llama(callback).await {
            Ok(()) => {
                let _ = tx.send(LlamaProgressEvent::Complete).await;
            }
            Err(e) => {
                let _ = tx
                    .send(LlamaProgressEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|event| {
        let (event_type, data) = match &event {
            LlamaProgressEvent::Progress { .. } => (
                "progress",
                serde_json::to_string(&event).unwrap_or_default(),
            ),
            LlamaProgressEvent::Complete => ("complete", "{}".to_string()),
            LlamaProgressEvent::Error { .. } => {
                ("error", serde_json::to_string(&event).unwrap_or_default())
            }
        };
        Ok(Event::default().event(event_type).data(data))
    });

    Sse::new(stream)
}

/// Set up the Python fast-download helper environment.
pub async fn setup_python(State(state): State<AppState>) -> Result<Json<()>, HttpError> {
    state.setup.setup_python_env().await?;
    Ok(Json(()))
}

/// SSE progress events for llama installation.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum LlamaProgressEvent {
    #[serde(rename_all = "camelCase")]
    Progress {
        downloaded: u64,
        total: u64,
    },
    Complete,
    #[serde(rename_all = "camelCase")]
    Error {
        message: String,
    },
}

/// Optional request body for [`build_llama_from_source`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildLlamaRequest {
    /// Acceleration backend override. If omitted, auto-detection is used.
    /// Valid values: `"metal"`, `"cuda"`, `"vulkan"`, `"cpu"`.
    pub acceleration: Option<String>,
}

/// Build llama.cpp from source with SSE progress streaming.
///
/// Returns a server-sent event stream. Named event types:
/// - `phase_started`: `{ "type": "phase_started", "phase": "<phase>" }`
/// - `progress`: `{ "type": "progress", "current": <n>, "total": <n> }`
/// - `log`: `{ "type": "log", "message": "<text>" }`
/// - `phase_completed`: `{ "type": "phase_completed", "phase": "<phase>" }`
/// - `completed`: `{ "type": "completed", "version": "<ver>", "acceleration": "<accel>" }`
/// - `failed`: `{ "type": "failed", "message": "<error>" }`
pub async fn build_llama_from_source(
    Json(req): Json<BuildLlamaRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let (tx, rx) = tokio::sync::mpsc::channel::<BuildEvent>(64);

    tokio::spawn(async move {
        let llama_dir = match llama_cpp_dir() {
            Ok(p) => p,
            Err(e) => {
                let _ = tx
                    .send(BuildEvent::Failed {
                        message: e.to_string(),
                    })
                    .await;
                return;
            }
        };
        let server_path = match llama_server_path() {
            Ok(p) => p,
            Err(e) => {
                let _ = tx
                    .send(BuildEvent::Failed {
                        message: e.to_string(),
                    })
                    .await;
                return;
            }
        };

        let acceleration = match req.acceleration.as_deref() {
            Some("metal") => Acceleration::Metal,
            Some("cuda") => Acceleration::Cuda,
            Some("vulkan") => Acceleration::Vulkan,
            Some("cpu") => Acceleration::Cpu,
            _ => match detect_optimal_acceleration() {
                Ok(a) => a,
                Err(e) => {
                    let _ = tx
                        .send(BuildEvent::Failed {
                            message: e.to_string(),
                        })
                        .await;
                    return;
                }
            },
        };

        if let Err(e) =
            run_llama_source_build(acceleration, llama_dir, server_path, tx.clone()).await
        {
            let _ = tx
                .send(BuildEvent::Failed {
                    message: e.to_string(),
                })
                .await;
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(build_event_to_sse);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

fn build_event_to_sse(event: BuildEvent) -> Result<Event, Infallible> {
    let event_type = match &event {
        BuildEvent::PhaseStarted { .. } => "phase_started",
        BuildEvent::Log { .. } => "log",
        BuildEvent::Progress { .. } => "progress",
        BuildEvent::PhaseCompleted { .. } => "phase_completed",
        BuildEvent::Completed { .. } => "completed",
        BuildEvent::Failed { .. } => "failed",
    };
    let data = serde_json::to_string(&event).unwrap_or_default();
    Ok(Event::default().event(event_type).data(data))
}
