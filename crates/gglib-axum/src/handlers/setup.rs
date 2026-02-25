//! Setup handlers - first-run wizard system status and provisioning.

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use serde::Serialize;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_gui::setup::SetupStatus;

/// Get the full system setup status for the first-run wizard.
pub async fn status(State(state): State<AppState>) -> Result<Json<SetupStatus>, HttpError> {
    Ok(Json(state.gui.get_setup_status().await?))
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
    let gui = state.gui.clone();

    tokio::spawn(async move {
        let tx_progress = tx.clone();
        let callback: Box<dyn Fn(u64, u64) + Send + Sync> =
            Box::new(move |downloaded: u64, total: u64| {
                // Best-effort send; if the receiver dropped, ignore
                let _ = tx_progress.try_send(LlamaProgressEvent::Progress {
                    downloaded,
                    total,
                });
            });

        match gui.install_llama(callback).await {
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
            LlamaProgressEvent::Progress { .. } => {
                ("progress", serde_json::to_string(&event).unwrap_or_default())
            }
            LlamaProgressEvent::Complete => {
                ("complete", "{}".to_string())
            }
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
    state.gui.setup_python_env().await?;
    Ok(Json(()))
}

/// SSE progress events for llama installation.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum LlamaProgressEvent {
    #[serde(rename_all = "camelCase")]
    Progress { downloaded: u64, total: u64 },
    Complete,
    #[serde(rename_all = "camelCase")]
    Error { message: String },
}
