//! Setup handlers - first-run wizard system status and provisioning.

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::StreamExt;
use futures_util::stream::Stream;
use serde::Serialize;

use crate::dto::diagnostics::{
    AccelerationDto, DiagnosticsDto, FastDownloadsDto, RecommendationDto, ResolvedPathsDto,
};
use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::setup::SetupStatus;
use gglib_core::paths::{llama_cpp_dir, llama_server_path};
use gglib_runtime::llama::{
    Acceleration, BuildEvent, LlamaProgressEvent, LlamaStatus, LlamaUpdateCheck, UninstallOutcome,
    llama_status, llama_update_check, run_llama_update, uninstall_llama, update_acceleration,
};

/// Get the full system setup status for the first-run wizard.
pub(crate) async fn status(State(state): State<AppState>) -> Result<Json<SetupStatus>, HttpError> {
    Ok(Json(state.setup.get_status().await?))
}

/// Install llama.cpp pre-built binaries with SSE progress streaming.
///
/// Streams [`LlamaProgressEvent`] verbatim — one SSE event name per variant,
/// the payload its JSON — exactly as [`update_llama`] streams [`BuildEvent`].
/// The browser reads the payload's `type`; the event name is a convenience.
///
/// This route used to declare a private three-variant event type of its own
/// and adapt a byte-counting callback into it, which is why it could report
/// neither the phase nor the speed the runtime already knew.
pub(crate) async fn install_llama(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let (tx, rx) = tokio::sync::mpsc::channel::<LlamaProgressEvent>(64);
    let setup = state.setup.clone();

    tokio::spawn(async move {
        // The pipeline reports failure by returning, leaving the wording to
        // whoever owns the channel. This is that owner.
        if let Err(e) = setup.install_llama(tx.clone()).await {
            let _ = tx
                .send(LlamaProgressEvent::Failed {
                    message: e.to_string(),
                })
                .await;
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(install_event_to_sse);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

fn install_event_to_sse(event: LlamaProgressEvent) -> Result<Event, Infallible> {
    let event_type = match &event {
        LlamaProgressEvent::PhaseStarted { .. } => "phase_started",
        LlamaProgressEvent::Progress { .. } => "progress",
        LlamaProgressEvent::PhaseCompleted { .. } => "phase_completed",
        LlamaProgressEvent::Completed { .. } => "completed",
        LlamaProgressEvent::Failed { .. } => "failed",
    };
    let data = serde_json::to_string(&event).unwrap_or_default();
    Ok(Event::default().event(event_type).data(data))
}

/// Set up the Python fast-download helper environment.
pub(crate) async fn setup_python(State(state): State<AppState>) -> Result<Json<()>, HttpError> {
    state.setup.setup_python_env().await?;
    Ok(Json(()))
}

/// Remove the fast-download helper environment — downloads revert to native
/// HTTP, which is a speed change, not a loss of capability.
pub(crate) async fn disable_fast_downloads(
    State(state): State<AppState>,
) -> Result<Json<DisableFastDownloadsResponse>, HttpError> {
    Ok(Json(DisableFastDownloadsResponse {
        removed: state.setup.remove_python_env()?,
    }))
}

/// Whether disabling actually removed anything, so the GUI can say
/// "disabled" rather than claiming a change that did not happen.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DisableFastDownloadsResponse {
    pub removed: bool,
}

/// System diagnostics: dependency matrix, resolved paths, detected
/// acceleration and accelerator state — `gglib config check-deps`, `paths`
/// and `fast-downloads status` in one response.
pub(crate) async fn diagnostics(
    State(state): State<AppState>,
) -> Result<Json<DiagnosticsDto>, HttpError> {
    let d = state.setup.diagnostics()?;

    Ok(Json(DiagnosticsDto {
        dependencies: d.dependencies.iter().map(Into::into).collect(),
        paths: ResolvedPathsDto::from(d.paths),
        acceleration: AccelerationDto {
            detected: d.acceleration.detected,
            detection_error: d.acceleration.detection_error,
        },
        fast_downloads: FastDownloadsDto {
            provisioned: d.fast_downloads.provisioned,
            env_dir: d.fast_downloads.env_dir,
            legacy_path: d.fast_downloads.legacy_path,
            builder: d.fast_downloads.builder,
            available_builder: d.fast_downloads.available_builder,
            error: d.fast_downloads.error,
        },
    }))
}

/// What llama.cpp install is present, if any — the GUI face of
/// `gglib config llama status`.
///
/// Local and cheap (no network), unlike [`check_llama_updates`].
pub(crate) async fn llama_status_handler() -> Result<Json<LlamaStatus>, HttpError> {
    // Probing spawns `llama-server --version` on a cold cache — blocking work
    // that does not belong on an async worker.
    tokio::task::spawn_blocking(llama_status)
        .await
        .map_err(|e| HttpError::Internal(format!("Status task panicked: {e}")))?
        .map(Json)
        .map_err(|e| HttpError::Internal(e.to_string()))
}

/// How far behind upstream the llama.cpp checkout is — the GUI face of
/// `gglib config llama check-updates`.
///
/// POST rather than GET because it runs `git fetch`: it mutates the local
/// checkout's remote refs and takes network time, so it belongs behind an
/// explicit action rather than something a page can issue on load.
pub(crate) async fn check_llama_updates() -> Result<Json<LlamaUpdateCheck>, HttpError> {
    llama_update_check()
        .await
        .map(Json)
        .map_err(|e| HttpError::Internal(e.to_string()))
}

/// Remove the llama.cpp installation — the GUI face of
/// `gglib config llama uninstall --force`.
///
/// The confirmation is the GUI's to run; by the time this is called the
/// decision is made.
pub(crate) async fn uninstall_llama_handler() -> Result<Json<UninstallOutcome>, HttpError> {
    if UPDATE_IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(HttpError::Conflict(
            "A llama.cpp build is running — uninstalling now would delete the checkout out \
             from under it. Wait for it to finish."
                .into(),
        ));
    }

    uninstall_llama()
        .await
        .map(Json)
        .map_err(|e| HttpError::Internal(e.to_string()))
}

/// Pull upstream and rebuild llama.cpp, streaming [`BuildEvent`]s over SSE —
/// the GUI face of `gglib config llama update`.
///
/// Rebuilds with the acceleration the current build recorded, so an update
/// cannot silently change backend. Preflight failures are reported as a
/// `failed` event rather than an HTTP status: by the time they are known the
/// response has already committed to being a stream.
/// One llama.cpp build at a time, process-wide.
///
/// Two concurrent builds share a source checkout and a binary destination, so
/// the second corrupts the first. The GUI disables its own button, which does
/// nothing about a second browser tab or a second client.
static UPDATE_IN_FLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) async fn update_llama()
-> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let (tx, rx) = tokio::sync::mpsc::channel::<BuildEvent>(64);

    // Claim the slot before spawning; release it when the task ends whatever
    // way it ends.
    let claimed = UPDATE_IN_FLIGHT
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_ok();

    if !claimed {
        let _ = tx.try_send(BuildEvent::Failed {
            message: "A llama.cpp build is already running.".to_string(),
        });
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(build_event_to_sse);
        return Sse::new(stream).keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(30))
                .text("ping"),
        );
    }

    tokio::spawn(async move {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                UPDATE_IN_FLIGHT.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let _guard = Guard;

        async fn preflight()
        -> anyhow::Result<(Acceleration, std::path::PathBuf, std::path::PathBuf)> {
            let llama_dir = llama_cpp_dir().map_err(|e| anyhow::anyhow!("{e}"))?;
            let server_path = llama_server_path().map_err(|e| anyhow::anyhow!("{e}"))?;
            if !llama_dir.exists() {
                anyhow::bail!(
                    "llama.cpp source checkout not found. A prebuilt install has no repository \
                     to update — reinstall from source first."
                );
            }
            Ok((update_acceleration()?, llama_dir, server_path))
        }

        let (acceleration, llama_dir, server_path) = match preflight().await {
            Ok(v) => v,
            Err(e) => {
                let _ = tx
                    .send(BuildEvent::Failed {
                        message: e.to_string(),
                    })
                    .await;
                return;
            }
        };

        if let Err(e) = run_llama_update(acceleration, llama_dir, server_path, tx.clone()).await {
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

/// A hardware-sized model suggestion — the shortlist `gglib up` picks from,
/// sized against this machine's memory.
///
/// Returns `null` when nothing in the shortlist fits. That is a real answer,
/// not an error: a machine too small for the smallest candidate needs to be
/// told so, not handed a recommendation it cannot run.
pub(crate) async fn recommend_model(
    State(state): State<AppState>,
) -> Result<Json<Option<RecommendationDto>>, HttpError> {
    Ok(Json(state.setup.recommend_model().map(Into::into)))
}
