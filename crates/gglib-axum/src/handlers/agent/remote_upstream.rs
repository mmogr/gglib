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
use gglib_runtime::FarMachine;

use super::AgentChatRequest;
use crate::{error::HttpError, handlers::port_utils::validate_port, state::AppState};

/// Where the completion adapter points, and with what.
pub(super) struct Upstream {
    /// `http://127.0.0.1:<port>`, without the `/v1`.
    pub base_url: String,
    /// The far machine on the remote path — its key and the fingerprint it
    /// is known by; nothing locally.
    pub far_machine: Option<FarMachine>,
    /// Resolved locally; passthrough for the remote, whose proxy resolves.
    pub model_context: ModelContext,
    /// The name that goes in the body's `model` field.
    ///
    /// Locally `None` is the ordinary case and means "whatever llama-server
    /// loaded". Remotely it is always `Some`, because [`remote_model`] has
    /// already refused the request that named nothing — the two cases read
    /// the same field and mean opposite things by an absence, so the
    /// decision is taken here rather than left for the handler.
    pub model: Option<String>,
}

/// The far machine's model name, or the refusal that says why there is none.
///
/// Only the remote path asks: locally an absent model is legitimate and means
/// something else entirely.
///
/// # Errors
///
/// `400` when the body named no model, or named one that is nothing but
/// whitespace — the same absence, and the same empty model downstream.
fn remote_model(req: &AgentChatRequest) -> Result<String, HttpError> {
    req.model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            HttpError::BadRequest(
                "no model named for the remote machine — name one it serves (`gglib model list` \
                 there is the list). This machine's default is not sent, because that machine may \
                 not have it"
                    .to_owned(),
            )
        })
}

/// Resolve the upstream for one request.
///
/// # Errors
///
/// Locally, whatever `validate_port` says. Remotely, `409` when this machine
/// is not connected, or is connected but holds no key — both are things
/// `gglib remote connect` fixes, and the message says so — and `400` when the
/// body named no model, which nothing downstream can fix.
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
            far_machine: None,
            model_context,
            model: req.model.clone(),
        });
    }

    // Before the connection is read: the request's own shape is settled
    // before this machine's state is, so a body that names no model is a
    // `400` whether or not a tunnel happens to be up.
    let model = remote_model(req)?;

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
        .remote_pairing
        .map(|stored| stored.api_key)
        .ok_or_else(|| {
            HttpError::Conflict(
                "connected to a remote machine, but this one holds no key for it — pair again \
                 with the full `<ticket>-<code>` string"
                    .to_owned(),
            )
        })?;
    Ok(Upstream {
        base_url: format!("http://127.0.0.1:{}", connection.port),
        // The fingerprint travels with the key because only this function
        // knows both: the request that fails on a rotated key comes back to
        // the adapter, which by then has no way to ask who was asked.
        far_machine: Some(FarMachine {
            key,
            fingerprint: connection.ticket_fingerprint,
        }),
        model_context: ModelContext::passthrough(),
        model: Some(model),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(json: &str) -> AgentChatRequest {
        serde_json::from_str(json).expect("parses")
    }

    /// The whole defect in one assertion: a body with `remote` and no `model`
    /// used to travel, arriving at the far proxy as `"model": ""`.
    #[test]
    fn a_remote_request_naming_no_model_is_refused_here() {
        let err = remote_model(&req(r#"{"port":9000,"messages":[],"remote":true}"#))
            .expect_err("no model named");
        assert!(matches!(err, HttpError::BadRequest(_)), "got {err:?}");
        assert!(
            err.to_string().contains("no model named"),
            "the message has to name the real problem, got: {err}"
        );
    }

    /// A field holding only spaces is the same absence, and `trim` downstream
    /// would otherwise turn it into the same empty model.
    #[test]
    fn a_model_of_only_whitespace_is_no_model_at_all() {
        assert!(
            remote_model(&req(
                r#"{"port":9000,"messages":[],"remote":true,"model":"  "}"#
            ))
            .is_err()
        );
    }

    #[test]
    fn a_named_model_is_forwarded_trimmed() {
        assert_eq!(
            remote_model(&req(
                r#"{"port":9000,"messages":[],"remote":true,"model":" qwen3 "}"#
            ))
            .expect("a name"),
            "qwen3"
        );
    }
}
