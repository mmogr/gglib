//! A fake far proxy on a loopback port, and a `FarProxy` at it: what each
//! request it saw carried, and the answer it was told to give. Shared by the
//! chats' and the models' tests.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use gglib_app_services::{FarCredentials, FarProxy};
use tokio::sync::{Notify, mpsc};
use tokio_stream::wrappers::ReceiverStream;

pub(super) const KEY: &str = "sk-far-key-for-this-device";

/// One request the far proxy saw.
#[derive(Debug, Clone)]
pub(super) struct Seen {
    pub(super) method: String,
    pub(super) uri: String,
    pub(super) bearer: Option<String>,
    pub(super) body: String,
}

/// What the far proxy answers, and what it saw.
pub(super) struct Fake {
    pub(super) seen: Mutex<Vec<Seen>>,
    pub(super) status: Mutex<u16>,
    pub(super) body: Mutex<String>,
    pub(super) retry_after: Mutex<Option<&'static str>>,
    /// Holds a run's stream after its first frame until notified.
    pub(super) release: Notify,
}

async fn answer(State(fake): State<Arc<Fake>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, usize::MAX).await.unwrap();
    fake.seen.lock().unwrap().push(Seen {
        method: parts.method.to_string(),
        uri: parts.uri.to_string(),
        bearer: parts
            .headers
            .get(header::AUTHORIZATION)
            .map(|v| v.to_str().unwrap().to_owned()),
        body: String::from_utf8_lossy(&body).into_owned(),
    });
    if parts.uri.path().ends_with("/events") && *fake.status.lock().unwrap() == 200 {
        let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(4);
        let fake = Arc::clone(&fake);
        tokio::spawn(async move {
            let frame = |text: &str| Ok(Bytes::from(text.to_owned()));
            tx.send(frame("id: 1\ndata: {\"n\":1}\n\n")).await.unwrap();
            fake.release.notified().await;
            tx.send(frame("id: 2\ndata: {\"n\":2}\n\n")).await.unwrap();
            tx.send(frame("event: run\ndata: {\"status\":\"completed\"}\n\n"))
                .await
                .unwrap();
        });
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(ReceiverStream::new(rx)))
            .unwrap();
    }
    let mut response = Response::builder()
        .status(*fake.status.lock().unwrap())
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(value) = *fake.retry_after.lock().unwrap() {
        response = response.header(header::RETRY_AFTER, value);
    }
    response
        .body(Body::from(fake.body.lock().unwrap().clone()))
        .unwrap()
}

/// The fingerprint of the machine the fake far proxy stands in for.
pub(super) const FINGERPRINT: &str = "0a1b2c3d4e5f";

/// A fake far proxy answering `status` with `body`, and a `FarProxy` at it.
pub(super) async fn far(status: u16, body: &str) -> (Arc<Fake>, FarProxy) {
    let fake = Arc::new(Fake {
        seen: Mutex::new(Vec::new()),
        status: Mutex::new(status),
        body: Mutex::new(body.to_owned()),
        retry_after: Mutex::new(None),
        release: Notify::new(),
    });
    let app = Router::new().fallback(answer).with_state(Arc::clone(&fake));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let credentials = FarCredentials {
        key: KEY.to_owned(),
        fingerprint: FINGERPRINT.to_owned(),
    };
    let client = FarProxy::new(&format!("http://127.0.0.1:{port}/v1"), &credentials).unwrap();
    (fake, client)
}

/// Whether a request carried this device's key as its bearer.
pub(super) fn carries_key(seen: &Seen) -> bool {
    seen.bearer.as_deref() == Some(format!("Bearer {KEY}").as_str())
}

pub(super) fn only(fake: &Fake) -> Seen {
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "{seen:?}");
    seen[0].clone()
}

pub(super) async fn read(response: Response) -> (StatusCode, String) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

pub(super) fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}
