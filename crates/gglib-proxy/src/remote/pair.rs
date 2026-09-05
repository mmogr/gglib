//! `POST /v1/remote/pair` — trade the one-time code for the key.
//!
//! The other half of `ServeHandle::grant_once`. The tunnel edge admits one
//! request bearing the code; this is where that request ends up, and the
//! tunnel's owner decides whether the code is the one this session minted.
//! The key crosses the encrypted hop in the response and is never shown on
//! a screen.
//!
//! Outside the bearer group, because it cannot demand the credential it
//! exists to hand out. Inside the Host guard like everything else.

use axum::{Json, body::Bytes, extract::State, http::StatusCode, response::IntoResponse};
use gglib_core::ports::PairingOutcome;
use serde::Deserialize;
use tracing::{info, warn};

use super::Tunnelled;
use crate::models::ErrorResponse;
use crate::server::AppState;

/// What the body has to carry.
#[derive(Debug, Deserialize)]
pub(crate) struct PairRequest {
    code: Option<String>,
}

/// Redeem a pairing code.
///
/// Every failure is the same flat 401 — wrong code, expired, spent, burned,
/// a body that does not parse, a proxy with no tunnel at all. Telling them
/// apart would tell a guesser which guess was close; the owner's three-attempt
/// burn is the whole defence and it needs no help from the error text. The
/// body is read raw and parsed here rather than through the `Json`
/// extractor, whose own rejection would be a 400 that says what was wrong.
pub(crate) async fn handle_remote_pair(
    State(state): State<AppState>,
    tunnelled: Option<axum::Extension<Tunnelled>>,
    body: Bytes,
) -> impl IntoResponse {
    let peer = tunnelled
        .as_ref()
        .and_then(|axum::Extension(t)| t.peer.clone());
    let code = serde_json::from_slice::<PairRequest>(&body)
        .ok()
        .and_then(|b| b.code);
    let outcome = match (state.remote_gateway(), code) {
        (Some(gateway), Some(code)) if !code.trim().is_empty() => {
            gateway.redeem_pairing_code(code.trim(), peer.as_deref())
        }
        _ => PairingOutcome::Rejected,
    };
    match outcome {
        PairingOutcome::Granted(api_key) => {
            info!(
                peer = peer.as_deref().unwrap_or("?"),
                "a device redeemed the pairing code and now holds the API key"
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({ "api_key": api_key })),
            )
                .into_response()
        }
        PairingOutcome::Rejected => {
            warn!(
                peer = peer.as_deref().unwrap_or("?"),
                "refused a pairing attempt"
            );
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::with_code(
                    "That pairing code was not accepted. Run `gglib remote enable` on the \
                     serving machine for a fresh one.",
                    "invalid_request_error",
                    "invalid_pairing_code",
                )),
            )
                .into_response()
        }
    }
}
