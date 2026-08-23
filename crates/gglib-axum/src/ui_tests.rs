//! Tests for [`super`] — the compiled-in dashboard's HTTP contract.
//!
//! These run against a committed three-file fixture rather than the real
//! `web_ui/`, so they assert the same thing in a fresh checkout as in a tree
//! that has run `npm run build`. Nothing here depends on
//! [`super::has_embedded_ui`].

use super::*;

use axum::body::to_bytes;

mod fixture {
    #![allow(unreachable_pub)]

    use rust_embed::RustEmbed;

    /// A stand-in for a Vite build: a shell plus two content-hashed assets.
    #[derive(RustEmbed)]
    #[folder = "tests/fixtures/ui"]
    pub(crate) struct Ui;

    /// A folder that does not exist.
    ///
    /// This is the guard for the constraint that shapes the whole design:
    /// `web_ui/` is gitignored and absent from a fresh checkout, and without
    /// `allow_missing` the derive is a hard compile error. If anyone removes
    /// the attribute, this file stops compiling — which is the point.
    #[derive(RustEmbed)]
    #[folder = "tests/fixtures/does-not-exist"]
    #[allow_missing = true]
    pub(crate) struct Empty;
}

/// Read a response's body, and its headers, into something assertable.
async fn parts(response: Response) -> (StatusCode, HeaderMap, String) {
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn header(headers: &HeaderMap, name: axum::http::HeaderName) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn root_serves_the_shell() {
    let (status, headers, body) = parts(respond::<fixture::Ui>("/", &HeaderMap::new())).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        header(&headers, header::CONTENT_TYPE).starts_with("text/html"),
        "the shell must be typed as HTML"
    );
    assert!(body.contains("FIXTURE_SHELL"), "body was {body:?}");
}

#[tokio::test]
async fn a_hashed_asset_is_typed_and_cached_forever() {
    let (status, headers, body) = parts(respond::<fixture::Ui>(
        "/assets/app-deadbeef.js",
        &HeaderMap::new(),
    ))
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        header(&headers, header::CONTENT_TYPE).contains("javascript"),
        "content-type was {:?}",
        header(&headers, header::CONTENT_TYPE)
    );
    // Vite renames on content change, so an immutable asset can never go
    // stale — and without this the ETag still costs a request per asset per
    // page load.
    assert!(
        header(&headers, header::CACHE_CONTROL).contains("immutable"),
        "cache-control was {:?}",
        header(&headers, header::CACHE_CONTROL)
    );
    assert!(body.contains("FIXTURE_SCRIPT"));
}

#[tokio::test]
async fn stylesheets_get_their_own_type() {
    let (_, headers, _) = parts(respond::<fixture::Ui>(
        "/assets/app-deadbeef.css",
        &HeaderMap::new(),
    ))
    .await;

    assert!(
        header(&headers, header::CONTENT_TYPE).starts_with("text/css"),
        "content-type was {:?}",
        header(&headers, header::CONTENT_TYPE)
    );
}

/// The shell must revalidate, or a new build is never picked up by a browser
/// that already has the old one.
#[tokio::test]
async fn the_shell_is_not_cached_immutably() {
    let (_, headers, _) = parts(respond::<fixture::Ui>("/", &HeaderMap::new())).await;

    assert_eq!(header(&headers, header::CACHE_CONTROL), "no-cache");
}

#[tokio::test]
async fn a_matching_etag_gets_304_and_no_body() {
    let (_, first, _) = parts(respond::<fixture::Ui>(
        "/assets/app-deadbeef.js",
        &HeaderMap::new(),
    ))
    .await;
    let etag = header(&first, header::ETAG);
    assert!(!etag.is_empty(), "the first response must carry an ETag");

    let mut request_headers = HeaderMap::new();
    request_headers.insert(header::IF_NONE_MATCH, etag.parse().unwrap());

    let (status, headers, body) = parts(respond::<fixture::Ui>(
        "/assets/app-deadbeef.js",
        &request_headers,
    ))
    .await;

    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty(), "a 304 carries no body, got {body:?}");
    // RFC 7232: the validator stays on the 304.
    assert_eq!(header(&headers, header::ETAG), etag);
}

#[tokio::test]
async fn a_stale_etag_gets_the_asset() {
    let mut request_headers = HeaderMap::new();
    request_headers.insert(header::IF_NONE_MATCH, "\"not-the-hash\"".parse().unwrap());

    let (status, _, body) = parts(respond::<fixture::Ui>(
        "/assets/app-deadbeef.js",
        &request_headers,
    ))
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("FIXTURE_SCRIPT"));
}

#[tokio::test]
async fn an_unknown_client_route_falls_back_to_the_shell() {
    let (status, headers, body) = parts(respond::<fixture::Ui>(
        "/models/whatever",
        &HeaderMap::new(),
    ))
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(header(&headers, header::CONTENT_TYPE).starts_with("text/html"));
    assert!(body.contains("FIXTURE_SHELL"));
}

/// The one deliberate behaviour change from `ServeDir`. Returning the shell
/// for a missing script made the browser report a MIME-type error against
/// `text/html`, which says nothing about the real problem.
#[tokio::test]
async fn a_missing_asset_is_404_rather_than_the_shell() {
    let (status, headers, body) = parts(respond::<fixture::Ui>(
        "/assets/gone-000000.js",
        &HeaderMap::new(),
    ))
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        !header(&headers, header::CONTENT_TYPE).starts_with("text/html"),
        "a missing asset must not be answered with the SPA shell"
    );
    assert!(!body.contains("FIXTURE_SHELL"));
}

#[tokio::test]
async fn traversal_is_refused() {
    let (status, _, _) = parts(respond::<fixture::Ui>("/../Cargo.toml", &HeaderMap::new())).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A binary built with no frontend must serve nothing rather than panic or
/// fail to compile. This is what keeps `cli-cross-os` and `docs.yml` green.
#[tokio::test]
async fn an_empty_embed_serves_nothing() {
    let (status, _, _) = parts(respond::<fixture::Empty>("/", &HeaderMap::new())).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
