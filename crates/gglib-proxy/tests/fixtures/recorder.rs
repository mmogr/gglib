//! A hop that writes down what the tunnel edge sends, for a suite that puts
//! one between the edge and the proxy.
//!
//! The proxy reads each tunnel marker with `HeaderMap::get`, which returns the
//! first of several copies, so the proxy alone cannot show that only one
//! arrived. This hop sees the whole header block, duplicates included, and
//! passes the request on.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::Request;
use axum::http::HeaderMap;
use axum::http::header::{CONNECTION, CONTENT_LENGTH, HOST, TRANSFER_ENCODING};
use reqwest::Client;
use tokio::net::TcpListener;

/// Every header block the hop passed on, in the order they arrived.
pub(crate) type Seen = Arc<Mutex<Vec<HeaderMap>>>;

/// Listen on a loopback port and forward every request to `upstream`,
/// recording its headers first. Returns the hop's base URL.
///
/// The hop rewrites only what belongs to its own connection: the authority
/// and the framing. Everything else, the markers and the bearer included,
/// reaches `upstream` as the edge wrote it.
pub(crate) async fn spawn_recorder(upstream: String) -> (String, Seen) {
    let seen = Seen::default();
    let log = Arc::clone(&seen);
    let hop = Router::new().fallback(move |request: Request| {
        let (log, upstream) = (Arc::clone(&log), upstream.clone());
        async move {
            let (parts, body) = request.into_parts();
            log.lock().unwrap().push(parts.headers.clone());
            let mut headers = parts.headers;
            for own in [HOST, CONNECTION, CONTENT_LENGTH, TRANSFER_ENCODING] {
                headers.remove(own);
            }
            let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
            let answer = Client::new()
                .request(parts.method, format!("{upstream}{}", parts.uri))
                .headers(headers)
                .body(body)
                .send()
                .await
                .expect("the upstream answers the hop");
            (answer.status(), answer.bytes().await.unwrap_or_default())
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, hop).await.ok() });
    (url, seen)
}
