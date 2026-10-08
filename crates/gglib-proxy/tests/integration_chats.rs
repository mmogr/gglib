//! `/v1/chats` on the real proxy: a paired device lists and opens the hub's
//! chats, and nothing else reaches them.
//!
//! The chats are a stand-in (`fixtures::chats`) that counts its calls,
//! because what is under test is the door: who passes, the bodies and the
//! error shape. What the list holds is the port's, tested where it lives.
//!
//! The door is one layer over one router group, which holds `/v1/attachments`
//! too, so the three tests of who passes run over all four routes here and
//! nowhere else. What an upload or a fetch answers is in
//! `integration_attachments.rs`.

use std::sync::Arc;

use gglib_core::domain::AttachmentId;
use gglib_core::ports::HubChatsPort;
use reqwest::{Client, RequestBuilder, StatusCode};

mod fixtures;
use fixtures::chats::{FakeChats, OPEN_ID, listed, opened, png, serve};
use fixtures::remote::from_device;
use fixtures::runs::{code, json};
use fixtures::tunnel::{DEVICE, DEVICE_KEY, PROXY_KEY, get, spawn_proxy_holding, tunnel_to};

/// Both chat routes, as a client reaches them.
fn chat_routes(base: &str) -> Vec<RequestBuilder> {
    let client = Client::new();
    vec![
        client.get(format!("{base}/v1/chats")),
        client.get(format!("{base}/v1/chats/{OPEN_ID}")),
    ]
}

/// An image, as a client sends one.
fn upload(base: &str) -> RequestBuilder {
    Client::new()
        .post(format!("{base}/v1/attachments"))
        .body(png(640, 480, 0))
}

/// Every route behind the door: the chats, an upload, and a fetch of an id
/// no image has.
fn routes(base: &str) -> Vec<RequestBuilder> {
    let unknown = AttachmentId::of(b"never uploaded");
    let mut routes = chat_routes(base);
    routes.push(upload(base));
    routes.push(Client::new().get(format!("{base}/v1/attachments/{unknown}")));
    routes
}

#[tokio::test]
async fn a_named_device_lists_the_hubs_chats() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let request = from_device(Client::new().get(format!("{base}/v1/chats")));
    let (status, body) = json(request.send().await.unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, serde_json::to_value(listed()).unwrap());
    cancel.cancel();
}

#[tokio::test]
async fn a_named_device_opens_a_chat_with_its_rows() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let request = from_device(Client::new().get(format!("{base}/v1/chats/{OPEN_ID}")));
    let (status, body) = json(request.send().await.unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, serde_json::to_value(opened()).unwrap());
    // A row's images travel as facts, by id: no bytes.
    let image = &body["messages"][0]["images"][0];
    assert_eq!(image["mime"], "image/png", "{body}");
    assert_eq!(image.as_object().unwrap().len(), 4, "{image}");
    cancel.cancel();
}

#[tokio::test]
async fn an_unknown_chat_is_404_not_found_and_echoes_nothing() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    for id in ["12345", "zzq-private-words"] {
        let request = from_device(Client::new().get(format!("{base}/v1/chats/{id}")));
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(code(&body), "not_found");
        assert!(!body.to_string().contains(id), "{body}");
    }
    cancel.cancel();
}

/// This machine reads its chats and sends its images at `/api`; a local
/// client, or one holding the key on a LAN bind, reaches none here.
#[tokio::test]
async fn a_request_not_tunnelled_from_a_named_device_is_refused() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    for request in routes(&base) {
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(code(&body), "device_not_named", "{body}");
    }
    // The device header without `Via` is not a tunnelled request.
    for request in routes(&base) {
        let request = request.header("x-modelpipe-device", DEVICE);
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(code(&body), "device_not_named", "{body}");
    }
    assert_eq!(chats.calls(), 0, "nothing reached the chats");
    cancel.cancel();
}

/// The routes are in the protected group: the key is asked for first, and
/// a tunnelled request that names no device is the gate's to refuse.
#[tokio::test]
async fn the_chats_and_the_images_sit_behind_the_bearer_and_the_device_gate() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(Some("secret123"), Some(Arc::clone(&chats))).await;
    for request in routes(&base) {
        let (status, body) = json(from_device(request).send().await.unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    }
    for request in routes(&base) {
        let request = request
            .bearer_auth("secret123")
            .header("via", "1.1 modelpipe");
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(code(&body), "device_not_paired", "{body}");
    }
    assert_eq!(chats.calls(), 0);
    for request in chat_routes(&base) {
        let request = from_device(request.bearer_auth("secret123"));
        assert_eq!(request.send().await.unwrap().status(), StatusCode::OK);
    }
    let sent = from_device(upload(&base)).bearer_auth("secret123");
    assert_eq!(sent.send().await.unwrap().status(), StatusCode::OK);
    cancel.cancel();
}

#[tokio::test]
async fn a_proxy_without_the_chats_answers_503_with_a_code() {
    let (base, cancel) = serve(None, None).await;
    for request in routes(&base) {
        let (status, body) = json(from_device(request).send().await.unwrap()).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(code(&body), "chats_unavailable");
    }
    cancel.cancel();
}

/// Through a real tunnel, the device the edge named for its key reads the
/// list.
#[tokio::test]
async fn a_device_through_the_tunnel_lists_the_chats() {
    let chats = Arc::new(FakeChats::default());
    let (proxy_url, cancel, _) = spawn_proxy_holding(
        PROXY_KEY,
        None,
        Some(Arc::clone(&chats) as Arc<dyn HubChatsPort>),
    )
    .await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    let answer = get(&format!("{base}/chats"), Some(DEVICE_KEY)).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.json(), serde_json::to_value(listed()).unwrap());

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}
