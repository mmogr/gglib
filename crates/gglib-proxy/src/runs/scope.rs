//! Who is asking: this machine, or the paired device the tunnel edge named.
//!
//! Read from the [`Tunnelled`] extension `remote_marker` sets, never from a
//! header directly. A request with no extension is `Local`. One with it is
//! the device the edge named; one the edge named no device for never gets
//! here, because `device_gate` refuses it first, and is refused here too.
//!
//! **What this cannot tell apart.** The edge dials this proxy like any other
//! client and presents the proxy's own key, so a client that holds that key
//! (or reaches a proxy that demands none) and writes the markers itself
//! arrives exactly as a tunnelled request does. Nothing in the proxy
//! distinguishes the two. Such a client is taken for the device it named,
//! and can read that device's replies, which the `Local` scope is refused.
//! That refusal is a courtesy of the API, not a boundary. Writing only the
//! device header, without `Via`, changes nothing: the request stays `Local`.

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use gglib_core::ports::RunScope;

use crate::models::ErrorResponse;
use crate::remote::Tunnelled;

/// The scope a request is served in.
pub(crate) struct Caller(pub(crate) RunScope);

/// The scope for what the marker read, or `None` for a tunnelled request
/// that names no device.
pub(crate) fn scope_of(tunnelled: Option<&Tunnelled>) -> Option<RunScope> {
    tunnelled.map_or(Some(RunScope::Local), |t| {
        t.device.as_deref().map(|d| RunScope::Device(d.to_owned()))
    })
}

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        scope_of(parts.extensions.get::<Tunnelled>())
            .map(Caller)
            .ok_or_else(|| {
                (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse::with_code(
                        "This request did not arrive on a device key.",
                        "invalid_request_error",
                        "device_not_paired",
                    )),
                )
                    .into_response()
            })
    }
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod scope_tests;
