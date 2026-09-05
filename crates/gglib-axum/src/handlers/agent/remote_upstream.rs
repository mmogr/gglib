//! Which upstream `POST /api/agent/chat` drives: a llama-server this daemon
//! started, or the machine on the other end of the remote tunnel.
//!
//! One decision, taken before anything else in the handler, because the two
//! cases differ in every input the adapter takes: the local case validates
//! the port against the servers this daemon owns and resolves the model
//! against this machine's catalog; the remote case takes the port the tunnel
//! bound, attaches the key from the pairing (ADR 0012, decision 7 — the
//! listener does not inject it), and shapes nothing, because the far proxy
//! runs its own pipeline over its own models.

use gglib_core::request_pipeline::{self, ModelContext};

use super::AgentChatRequest;
use crate::{error::HttpError, handlers::port_utils::validate_port, state::AppState};

/// Where the completion adapter points, and with what.
pub(super) struct Upstream {
    /// `http://127.0.0.1:<port>`, without the `/v1`.
    pub base_url: String,
    /// The far machine's key on the remote path; nothing locally.
    pub bearer: Option<String>,
    /// Resolved locally; passthrough for the remote, whose proxy resolves.
    pub model_context: ModelContext,
}

/// Resolve the upstream for one request.
///
/// # Errors
///
/// Locally, whatever `validate_port` says. Remotely, `409` when this machine
/// is not connected, or is connected but holds no key — both are things
/// `gglib remote connect` fixes, and the message says so.
pub(super) async fn resolve(
    state: &AppState,
    req: &AgentChatRequest,
) -> Result<Upstream, HttpError> {
    if !req.remote {
        validate_port(state, req.port).await?;
        let model_context =
            request_pipeline::resolve(state.catalog.as_ref(), req.model.as_deref()).await;
        return Ok(Upstream {
            base_url: format!("http://127.0.0.1:{}", req.port),
            bearer: None,
            model_context,
        });
    }

    let Some(connection) = state.remote.status().await.connected else {
        return Err(HttpError::Conflict(
            "not connected to a remote machine — `gglib remote connect` first".to_owned(),
        ));
    };
    // The core settings, not the GUI's `AppSettings`: the key is deliberately
    // absent from the shapes the settings panel reads.
    let key = state
        .core
        .settings()
        .get()
        .await
        .map_err(|e| HttpError::Internal(format!("could not read settings: {e}")))?
        .remote_api_key
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            HttpError::Conflict(
                "connected to a remote machine, but this one holds no key for it — pair again \
                 with the full `<ticket>-<code>` string"
                    .to_owned(),
            )
        })?;
    Ok(Upstream {
        base_url: format!("http://127.0.0.1:{}", connection.port),
        bearer: Some(key),
        model_context: ModelContext::passthrough(),
    })
}
