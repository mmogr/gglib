//! What `POST /api/agent/chat` does before its loop runs, and how it writes
//! each event: one place, so every caller of the agent loop resolves,
//! validates, limits and frames exactly as the chat route does.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, mpsc};

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_core::ports::{AdmissionLease, AgentGuardReporter, AgentLoopPort, RetryObserver};
use gglib_runtime::compose_agent_loop;

use super::AgentChatRequest;
use super::remote_upstream;
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
    /// The model the loop is counted under: the request's, or the one
    /// running on its port.
    pub(crate) model: String,
    /// The model each turn was made by, for its `turn_usage` event.
    pub(crate) made_by: MadeBy,
    /// The port and id of the local model the loop drives; remotely none.
    pub(crate) local_model: Option<(u16, i64)>,
    /// A run's hold on that model (`remote_upstream::hold`); `prepare` takes
    /// none.
    pub(crate) hold: Option<AdmissionLease>,
}

/// The model a run drives, and the paired device whose turn it answers,
/// which the loop does not know: stamped on each turn's usage before it is
/// logged.
pub(crate) struct MadeBy {
    pub(crate) model: String,
    pub(crate) quantization: Option<String>,
    /// Absent for this machine's own turns.
    pub(crate) device: Option<String>,
}

impl MadeBy {
    /// Name the model on a `turn_usage` event; any other passes unchanged.
    pub(crate) fn stamp(&self, event: &mut AgentEvent) {
        if let AgentEvent::TurnUsage(usage) = event {
            usage.model = Some(self.model.clone());
            usage.quantization.clone_from(&self.quantization);
            usage.device.clone_from(&self.device);
        }
    }
}

/// One slot of the agent semaphore, or `None` when every slot is taken.
pub(crate) fn take_permit(state: &AppState) -> Option<OwnedSemaphorePermit> {
    state.agent_semaphore.clone().try_acquire_owned().ok()
}

/// Resolve the upstream, validate the request, apply the configured limits
/// and compose the loop.
///
/// # Errors
///
/// Whatever [`remote_upstream::resolve`] refuses.
pub(crate) async fn prepare(
    state: &AppState,
    req: AgentChatRequest,
) -> Result<Prepared, HttpError> {
    // Local llama-server or the remote tunnel: settled first, because it
    // decides the port check, the model context and the bearer together.
    let upstream = remote_upstream::resolve(state, &req).await?;

    // Read before `tool_filter` consumes the request piecemeal, and before the
    // loop is composed: the two reasoning controls are the only sampling this
    // endpoint accepts, and they occupy the ladder's top rung.
    let sampling = req.sampling_layer();
    let tool_filter: Option<HashSet<String>> = req.tool_filter.map(|f| f.into_iter().collect());

    // Created before the loop is composed so the completion adapter can report
    // its retries onto the same stream the loop emits through — otherwise a
    // contended model is indistinguishable from a hung one for as long as the
    // retry budget lasts.
    let (tx, rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
    let retry_observer: Arc<dyn RetryObserver> = Arc::new(RetryNotice::new(tx.clone()));

    let model = upstream.counted_as.clone();
    let local_model = upstream.local_model;
    let agent_loop = compose_agent_loop(
        upstream.base_url,
        state.http_client.clone(),
        // `upstream`, not `req`: the two paths mean opposite things by an
        // absent model, and `resolve` is where that was already decided.
        upstream.model,
        upstream.model_context,
        state.mcp.clone(),
        tool_filter,
        // GUI chat runs in the same process as the embedded proxy; report its
        // reuse to the shared agent-path store behind `agent_usage`.
        Some(state.proxy.agent_metrics()),
        // And its guard decisions to the same process's ledger, which is the
        // only reason the agent path's trips are counted at all (#1091). The
        // name is `resolve`'s, not `req`'s: a request that named no model is
        // counted under the model actually running on the port.
        AgentGuardReporter {
            sink: state.proxy.agent_guard_sink(),
            model: upstream.counted_as,
        },
        Some(retry_observer),
        sampling,
        upstream.far_machine,
    );

    // Stagnation threshold is a persisted server-side setting, not a request
    // field; a settings-read failure falls back to the built-in default.
    let max_stagnation_steps = state
        .settings
        .get()
        .await
        .ok()
        .and_then(|s| s.max_stagnation_steps)
        .map(|v| v as usize);
    let config: AgentConfig = req
        .config
        .unwrap_or_default()
        .into_agent_config(max_stagnation_steps);

    Ok(Prepared {
        agent_loop,
        messages: req.messages,
        config,
        tx,
        rx,
        model,
        made_by: upstream.made_by,
        local_model,
        hold: None,
    })
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
            };
            serde_json::to_string(&typed_fallback).unwrap_or_else(|_| {
                r#"{"type":"error","message":"serialization failed"}"#.to_owned()
            })
        }
    }
}
