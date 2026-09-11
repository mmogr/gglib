//! `POST /v1/remote/pair` — trade the one-time code for the key.
//!
//! The other half of `ServeHandle::grant_once`. The tunnel edge admits one
//! request bearing the code; this is where that request ends up, and the
//! tunnel's owner decides whether the code is the one this session minted.
//! The key crosses the encrypted hop in the response and is never shown on
//! a screen.
//!
//! Outside the bearer group, because it cannot demand the credential it
//! exists to hand out. Inside the Host guard like everything else — and, for
//! tunnelled requests, behind one check of its own: a device that already
//! holds a key has no business here, and is refused before the code is looked
//! at. See [`handle_remote_pair`].

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
    /// What the device calls itself, when it says.
    ///
    /// Optional and ignored when absent, so a client built before per-device
    /// keys still pairs. It is a label for a person to read in
    /// the device list and is sent nowhere — the name the tunnel edge
    /// holds the key under is minted here, not accepted from the wire.
    #[serde(default)]
    name: Option<String>,
}

/// The longest label kept, in characters.
///
/// This value comes off the wire from a machine that has not paired yet, and
/// ends up in a settings row and then in terminal output. Nothing downstream
/// needs it to be long, and the only bound otherwise is axum's body limit.
const MAX_LABEL: usize = 64;

/// A label fit to store: invisible characters dropped, cut to [`MAX_LABEL`]
/// characters, then trimmed.
///
/// Dropped rather than escaped, and counted in characters rather than bytes,
/// because this is read by a person and rendered in two surfaces that have no
/// say in what arrives here. Nothing depends on the value, so a label that
/// loses its tail is a cosmetic loss and an unbounded one is not. See
/// [`is_invisible`] for what goes and why.
fn label(name: Option<String>) -> Option<String> {
    let cleaned: String = name?
        .chars()
        .filter(|c| !is_invisible(*c))
        .take(MAX_LABEL)
        .collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Characters a label has no use for and a terminal or a browser would act
/// on.
///
/// `char::is_control` alone covers only the C0/C1 range, which leaves the
/// ones that actually matter here: the bidirectional overrides, which let a
/// device make its listed name render as a different device's, and the line
/// and paragraph separators, which break a row across two. Dropped rather
/// than escaped, for the same reason the rest is.
fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{feff}')
}

/// Redeem a pairing code.
///
/// Every failure is the same flat 401 — wrong code, expired, spent, burned,
/// a body that does not parse, a proxy with no tunnel at all. Telling them
/// apart would tell a guesser which guess was close; the flat refusal needs no
/// help from the error text. The three-attempt burn is the defence *locally* —
/// over the tunnel a wrong code is a wrong bearer and modelpipe's edge refuses
/// it before this handler runs, so `Pairing::attempts` never increments. See
/// ADR 0012 decision 3, amended 2026-09-07, for what that leaves. The
/// body is read raw and parsed here rather than through the `Json`
/// extractor, whose own rejection would be a 400 that says what was wrong.
///
/// **A request the edge admitted on a device key never gets here.** This
/// route is outside the bearer group, so under `TokenPolicy::Named` a paired
/// device reaches it the way it reaches any other path — with its own valid
/// bearer, which is not the one-time grant this route is for. What that buys
/// a device that has been compromised is two things worth closing: three
/// wrong codes clear whatever invite is open, so a retired-but-not-yet-
/// forgotten laptop can burn every code a person types; and three guesses per
/// invite at a *second* identity, which would survive that laptop being
/// forgotten. Neither is something a device that already holds a key has any
/// reason to do, so the refusal costs nothing. It is the same flat 401 as
/// every other refusal here.
pub(crate) async fn handle_remote_pair(
    State(state): State<AppState>,
    tunnelled: Option<axum::Extension<Tunnelled>>,
    body: Bytes,
) -> impl IntoResponse {
    let peer = tunnelled
        .as_ref()
        .and_then(|axum::Extension(t)| t.peer.clone());
    let already_paired = tunnelled
        .as_ref()
        .is_some_and(|axum::Extension(t)| t.device.is_some());
    let request = serde_json::from_slice::<PairRequest>(&body).ok();
    let name = request.as_ref().and_then(|b| b.name.clone());
    let code = request.and_then(|b| b.code);
    let outcome = match (state.remote_gateway(), code) {
        _ if already_paired => PairingOutcome::Rejected,
        (Some(gateway), Some(code)) if !code.trim().is_empty() => {
            gateway.redeem_pairing_code(code.trim(), peer.as_deref(), label(name).as_deref())
        }
        _ => PairingOutcome::Rejected,
    };
    match outcome {
        PairingOutcome::Granted { key, device } => {
            info!(
                peer = peer.as_deref().unwrap_or("?"),
                device = device.as_str(),
                "a device redeemed the pairing code and now holds a key of its own"
            );
            // `api_key` keeps its name: two decoders pin it — gglib's own
            // `redeem.rs` and ggchat's `PairResponse` — and renaming it would
            // break the phone silently at runtime. `device_id` is added
            // beside it, which an older client ignores.
            (
                StatusCode::OK,
                Json(serde_json::json!({ "api_key": key, "device_id": device })),
            )
                .into_response()
        }
        PairingOutcome::Rejected => {
            warn!(
                peer = peer.as_deref().unwrap_or("?"),
                already_paired, "refused a pairing attempt"
            );
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::with_code(
                    "That pairing code was not accepted. Run \
                     `gglib remote enable --invite` on the serving machine for a fresh one.",
                    "invalid_request_error",
                    "invalid_pairing_code",
                )),
            )
                .into_response()
        }
    }
}
