//! The dashboard, compiled into the binary.
//!
//! The daemon used to find its frontend by probing `./web_ui/dist`, `./dist`,
//! `./web_ui/assets` and `./web_ui` — all relative to the **working
//! directory**. A release tarball extracted to `~/bin` and run from anywhere
//! else therefore served the API with no dashboard and no diagnostic. Worse,
//! the daemon is a singleton behind [`crate::DaemonLock`]: if the desktop app
//! started it (working directory `/` when launched from Finder), the dashboard
//! was off for every later client too, permanently.
//!
//! Compiling the assets in removes the question. It also makes the two
//! binaries symmetric — `gglib-app` already embeds this exact Vite output via
//! Tauri's `frontendDist` and `custom-protocol`.
//!
//! What is lost against `tower_http::services::ServeDir`, and why it is
//! acceptable: `Last-Modified`/`If-Modified-Since` (the `ETag` below subsumes
//! it), `Range` requests (nothing here streams media), and directory redirects
//! (there are no directories to index). Content type and conditional requests
//! are reimplemented.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

use crate::access::{DaemonAccess, host_guard};
use crate::state::AppState;
use gglib_core::CorsConfig;

mod embed {
    #![allow(unreachable_pub)]

    use rust_embed::RustEmbed;

    /// The Vite output from `npm run build`.
    ///
    /// `allow_missing` is load-bearing: without it a missing folder is a hard
    /// compile error, and `web_ui/` is gitignored and absent from a fresh
    /// checkout. `cargo test -p gglib-cli --no-run` (ci.yml's `cli-cross-os`)
    /// and `cargo doc --workspace --exclude gglib-app` (docs.yml) both build
    /// this crate with no frontend, and must keep working. With the attribute
    /// the asset set is simply empty and the daemon serves the API alone.
    #[derive(RustEmbed)]
    #[folder = "../../web_ui"]
    #[allow_missing = true]
    pub(crate) struct WebUi;
}

/// Whether this binary carries a dashboard build.
///
/// False for a binary built from a tree where `npm run build` had never run.
#[must_use]
pub fn has_embedded_ui() -> bool {
    embed::WebUi::iter().next().is_some()
}

/// The daemon router with the compiled-in dashboard as its fallback.
///
/// The embedded twin of [`crate::create_spa_router`]: same layering, same SPA
/// semantics, no filesystem.
///
/// # Example
/// ```no_run
/// # use std::sync::Arc;
/// # use gglib_axum::{AppState, CorsConfig, DaemonAccess};
/// # async fn example(state: AppState) {
/// let access = Arc::new(DaemonAccess::loopback());
/// let router = gglib_axum::create_embedded_spa_router(state, &CorsConfig::AllowAll, access);
/// # }
/// ```
pub fn create_embedded_spa_router(
    state: AppState,
    cors_config: &CorsConfig,
    access: Arc<DaemonAccess>,
) -> Router {
    // Same order as `create_spa_router`, and for the same reason: the Host
    // guard is layered *after* the fallback so it wraps the assets too. A
    // rebound page must not be able to load the dashboard at all.
    //
    // `get(..)` rather than `fallback(..)` so a non-GET to an unknown path
    // answers 405, matching what ServeDir did, instead of handing back HTML.
    crate::routes::base_router(state, cors_config, &access)
        .fallback_service(get(embedded_handler::<embed::WebUi>))
        .layer(middleware::from_fn_with_state(access, host_guard))
}

/// Axum entry point. The work is in [`respond`], which needs no runtime.
async fn embedded_handler<E: RustEmbed>(request: Request) -> Response {
    let path = request.uri().path().to_owned();
    respond::<E>(&path, request.headers())
}

/// Resolve one request against an embedded asset set.
///
/// Split from the handler so it is callable as a plain function from tests —
/// no runtime, no `AppState`, no bootstrap.
fn respond<E: RustEmbed>(path: &str, headers: &HeaderMap) -> Response {
    let rel = path.trim_start_matches('/');

    // The release impl matches keys by exact string so traversal cannot reach
    // outside the set, and the debug impl canonicalises. Rejecting `..`
    // outright costs nothing and does not rely on either staying true.
    if rel.split('/').any(|segment| segment == "..") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let key = if rel.is_empty() { "index.html" } else { rel };

    if let Some(response) = serve::<E>(key, headers) {
        return response;
    }

    // A miss whose last segment looks like a filename is a 404, not the SPA
    // shell. Handing `index.html` back for a stale `/assets/main-a1b2c3.js`
    // produces a confusing MIME-type console error instead of a clean 404.
    if key
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Client-side route: serve the shell. There is no router in the frontend
    // today, so in practice this is `/` and typos.
    serve::<E>("index.html", headers).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

/// Serve one asset by exact key, honouring `If-None-Match`.
fn serve<E: RustEmbed>(key: &str, headers: &HeaderMap) -> Option<Response> {
    let file = E::get(key)?;
    let etag = etag_of(&file.metadata.sha256_hash());

    // Content-hashed filenames can be cached forever; the shell must not be,
    // or a new build is never picked up. Without `immutable` the ETag still
    // costs a round-trip per asset on every page load.
    let cache_control = if key.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    let mut response_headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&etag) {
        response_headers.insert(header::ETAG, value);
    }
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );

    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|candidate| candidate.trim() == etag))
    {
        // RFC 7232: a 304 still carries the validator.
        return Some((StatusCode::NOT_MODIFIED, response_headers).into_response());
    }

    if let Ok(value) = HeaderValue::from_str(file.metadata.mimetype()) {
        response_headers.insert(header::CONTENT_TYPE, value);
    }

    // Borrowed in release (embedded `&'static [u8]`, no copy); owned in debug,
    // where rust-embed reads from disk.
    let body = match file.data {
        std::borrow::Cow::Borrowed(bytes) => Body::from(Bytes::from_static(bytes)),
        std::borrow::Cow::Owned(bytes) => Body::from(bytes),
    };

    Some((StatusCode::OK, response_headers, body).into_response())
}

/// A quoted, 128-bit hex validator. Half a SHA-256 is ample for a cache tag.
fn etag_of(hash: &[u8; 32]) -> String {
    use std::fmt::Write as _;

    let mut tag = String::with_capacity(34);
    tag.push('"');
    for byte in &hash[..16] {
        let _ = write!(tag, "{byte:02x}");
    }
    tag.push('"');
    tag
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;
