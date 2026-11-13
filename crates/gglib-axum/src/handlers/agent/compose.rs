//! What a run does before its loop runs, and how it writes each event: one
//! place, so every door a turn comes through validates, limits and frames
//! alike.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, mpsc};

use gglib_app_services::transcript::MadeBy;
use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::ModelRef;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_core::ports::{AgentGuardReporter, AgentLoopPort, RetryObserver};
use gglib_core::services::{AttachmentService, drawing_availability};
use gglib_mcp::{BUILTIN_PREFIX, DrawArm, DrawWith, DrawingTool, FILE_TOOLS};
use gglib_runtime::compose_agent_loop;

use super::loop_request::{LoopRequest, NamedLimits};
use super::remote_upstream::Upstream;
use super::retry_notice::RetryNotice;
use crate::error::HttpError;
use crate::state::AppState;

/// One request's loop, ready to run.
pub(crate) struct Prepared {
    pub(crate) agent_loop: Arc<dyn AgentLoopPort>,
    pub(crate) messages: Vec<AgentMessage>,
    pub(crate) config: AgentConfig,
    /// The loop's sender; the retry notices hold a clone of it.
    pub(crate) tx: mpsc::Sender<AgentEvent>,
    /// Where the loop's events arrive.
    pub(crate) rx: mpsc::Receiver<AgentEvent>,
    /// The model each turn was made by, for its `turn_usage` event. Its
    /// context's size is read once the run's model is loaded
    /// (`remote_upstream::endpoint`).
    pub(crate) made_by: MadeBy,
    /// The port and id of the local model the loop drives; otherwise none.
    pub(crate) local_model: Option<(u16, i64)>,
    /// The paired machine's model the loop drives, by its machine;
    /// otherwise none.
    pub(crate) far_model: Option<ModelRef>,
}

/// One slot of the agent semaphore, or `None` when every slot is taken or
/// a paired device's run waits in line for one (`slot_line`).
pub(crate) fn take_permit(state: &AppState) -> Option<OwnedSemaphorePermit> {
    state.agent_line.take()
}

/// The image tool, as a tool filter names it: qualified, so it can only ever
/// match the builtin. The bare name would also match an MCP server's tool of
/// that name (`FilteredToolExecutor` matches after the first `:`).
pub(crate) const DRAW_TOOL: &str = gglib_core::request_pipeline::DRAW_TOOL_NAME;

/// Refuse a request sent with Draw pressed that this machine cannot draw
/// for: its model on another machine, no image runtime, no usable image
/// model, or a chosen image model (`image_model`) that is gone, is not one
/// or lacks a file. Called before a slot is taken or anything is written. A
/// request without `draw` passes, unless it chose an image model: it is
/// offered no image tool at all, so the choice would be dropped in silence.
///
/// The one place every door's `draw` and `image_model` are judged: this
/// machine's turn, a device's turn and a chat run with builtins.
///
/// # Errors
///
/// `invalid_request` (400) for `image_model` without `draw`; otherwise 400
/// with the availability answer's own code (`drawing_unavailable`,
/// `model_not_found`, `not_an_image_model`, `image_model_incomplete`) and
/// its reason.
pub(crate) async fn refuse_unavailable_drawing(
    state: &AppState,
    req: &LoopRequest,
) -> Result<(), HttpError> {
    let refused = |code, message| HttpError::Coded {
        status: axum::http::StatusCode::BAD_REQUEST,
        code,
        message,
    };
    if !req.draw {
        if req.image_model.is_some() {
            return Err(refused(
                "invalid_request",
                "`image_model` goes with `draw`: it chooses what a message sent with Draw \
                 pressed draws with"
                    .to_owned(),
            ));
        }
        return Ok(());
    }
    let images = Some(state.images.as_ref());
    let answer = drawing_availability(images, req.far.is_some(), None, req.image_model).await;
    if answer.available {
        return Ok(());
    }
    Err(refused(
        drawing_code(answer.code.as_deref()),
        answer
            .reason
            .unwrap_or_else(|| "this machine cannot draw".to_owned()),
    ))
}

/// An availability answer's code as the refusal's: one of the four a
/// request to draw is refused with, and `drawing_unavailable` for an answer
/// that gives none or one not known here.
fn drawing_code(code: Option<&str>) -> &'static str {
    match code {
        Some("model_not_found") => "model_not_found",
        Some("not_an_image_model") => "not_an_image_model",
        Some("image_model_incomplete") => "image_model_incomplete",
        _ => "drawing_unavailable",
    }
}

/// A request's tool filter once Draw is counted: every tool stays every
/// tool, and a list gains exactly [`DRAW_TOOL`] when the message was sent
/// with Draw pressed. Without `draw` the filter is as it was; the image tool
/// is then kept out by the executor, which lists it only when armed.
pub(crate) fn with_drawing(
    mut filter: Option<HashSet<String>>,
    draw: bool,
) -> Option<HashSet<String>> {
    if let (Some(named), true) = (filter.as_mut(), draw) {
        named.insert(DRAW_TOOL.to_owned());
    }
    filter
}

/// gglib's own file tools ([`FILE_TOOLS`]), as a tool filter names them:
/// qualified, so they can only ever match the builtins, as [`DRAW_TOOL`] is.
pub(crate) fn file_tools() -> impl Iterator<Item = String> {
    FILE_TOOLS
        .iter()
        .map(|name| format!("{BUILTIN_PREFIX}{name}"))
}

/// A request's tool filter once the run's folder is counted: a list that
/// names any tool gains [`file_tools`] when the run has a folder, since
/// choosing the folder is what turns them on. `[]` stays `[]`: no tool at
/// all, for a model that calls none. Every tool stays every tool.
pub(crate) fn with_folder(
    mut filter: Option<HashSet<String>>,
    has_folder: bool,
) -> Option<HashSet<String>> {
    if let (Some(named), true) = (filter.as_mut(), has_folder)
        && !named.is_empty()
    {
        named.extend(file_tools());
    }
    filter
}

/// Apply the configured limits and compose `req`'s loop over `upstream`,
/// with `folder` as the root of gglib's own file tools: a canonical path,
/// or `None` for no file tools.
///
/// A loop on this machine's model goes through its endpoint
/// (`remote_upstream::endpoint`): its turns are any client's there, so the
/// endpoint's loop guard counts them and this loop reports its own guard's
/// decisions nowhere. A loop on the paired machine's model reports them to
/// this machine's ledger.
pub(super) async fn prepare_over(
    state: &AppState,
    req: LoopRequest,
    upstream: Upstream,
    folder: Option<PathBuf>,
) -> Prepared {
    // Read before `tool_filter` consumes the request piecemeal, and before the
    // loop is composed: the turn's own sampling, the ladder's top rung.
    let sampling = req.sampling_layer();
    let draw = req.draw;
    // The folder before Draw: a `[]` that Draw then names the image tool in
    // is still a model that calls no other tool.
    let named = req.tool_filter.map(|f| f.into_iter().collect());
    let tool_filter = with_drawing(with_folder(named, folder.is_some()), req.draw);
    // The image tool exists for this loop only when the message was sent
    // with Draw pressed; without it no filter, `null` included, reaches one.
    // The image model the person chose goes on the tool, for the request it
    // hands the driver, and into nothing the chat model is sent.
    let drawing = req.draw.then(|| {
        let store = AttachmentService::new(state.core.attachments().store());
        (
            DrawingTool::new(Arc::clone(&state.images), Arc::new(store))
                .drawing_with(DrawWith::model(req.image_model)),
            DrawArm::Fixed(true),
        )
    });

    // Created before the loop is composed so the completion adapter can report
    // its retries onto the same stream the loop emits through — otherwise a
    // contended model is indistinguishable from a hung one for as long as the
    // retry budget lasts.
    let (tx, rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
    let retry_observer: Arc<dyn RetryObserver> = Arc::new(RetryNotice::new(tx.clone()));

    let local_model = upstream.local_model;
    let far_model = upstream.far_model.clone();
    // A far run's guard decisions go to this process's ledger (#1091),
    // under the name that machine has for the model.
    let guard = reports_its_guard(&upstream.door).then(|| AgentGuardReporter {
        sink: state.proxy.agent_guard_sink(),
        model: upstream.counted_as,
    });
    let agent_loop = compose_agent_loop(
        upstream.base_url,
        state.http_client.clone(),
        upstream.model,
        upstream.model_context,
        state.mcp.clone(),
        tool_filter,
        folder,
        guard,
        Some(retry_observer),
        sampling,
        upstream.layers,
        upstream.door,
        state.core.attachments().store(),
        drawing,
    );

    let mut config = config_for(state, req.limits).await;
    // Pressing Draw is the person's explicit ask: the run's first reply
    // must be the call for the picture, offered that tool alone, and is
    // never left to the model to decide. The picture is asked for once:
    // the reply after it is offered no tools. Every door that composes a run
    // with `draw` comes through here: this machine's turns, a device's and
    // a chat run with builtins. A run on another machine's model was
    // refused `draw` before this.
    config.first_call = draw.then(DrawingTool::first_call);

    Prepared {
        agent_loop,
        messages: req.messages,
        config,
        tx,
        rx,
        made_by: upstream.made_by,
        local_model,
        far_model,
    }
}

/// Whether a loop through `door` reports its own guard's decisions to this
/// process's ledger: one on a paired machine's model does, which this
/// machine counts as it composed it. A loop through this machine's endpoint
/// does not: the endpoint's loop guard sees every request and counts what
/// it decides, and a count here would count each trip twice.
pub(super) fn reports_its_guard(door: &gglib_runtime::Door) -> bool {
    door.is_far()
}

/// The loop config a turn runs with: the limits its chat's settings name
/// (`hub_turn::plan`, for this machine's turn and a paired device's), and
/// for those they leave out this machine's stored ones
/// (`TurnLimits::resolve`). So a turn on a chat that names no iteration
/// limit runs with `max_tool_iterations`. A settings read that fails leaves
/// the built-in defaults.
///
/// Its own function so a test can read the config a planned turn runs with.
pub(crate) async fn config_for(state: &AppState, named: NamedLimits) -> AgentConfig {
    let settings = state.core.settings().get().await.ok();
    named.into_agent_config(settings.as_ref())
}

/// One event as the `data:` text of its SSE frame.
pub(crate) fn frame(event: &AgentEvent) -> String {
    match serde_json::to_string(event) {
        Ok(json) => json,
        Err(e) => {
            // Silently dropping a frame here would leave the client hanging
            // indefinitely — especially fatal if the failed event is
            // `FinalAnswer` or `Error`. Construct a typed fallback event so
            // the client always receives a terminal signal that is
            // structurally valid regardless of future AgentEvent changes.
            tracing::error!(error = %e, "agent: failed to serialise AgentEvent; emitting fallback error");
            let typed_fallback = AgentEvent::Error {
                message: "serialization failed".to_owned(),
                code: None,
            };
            serde_json::to_string(&typed_fallback).unwrap_or_else(|_| {
                r#"{"type":"error","message":"serialization failed"}"#.to_owned()
            })
        }
    }
}

#[cfg(test)]
#[path = "compose_draw_tests.rs"]
mod draw_tests;
#[cfg(test)]
#[path = "compose_drawn_picture_tests.rs"]
mod drawn_picture_tests;
#[cfg(test)]
#[path = "compose_endpoint_tests.rs"]
mod endpoint_tests;
#[cfg(test)]
#[path = "compose_folder_tests.rs"]
mod folder_tests;
