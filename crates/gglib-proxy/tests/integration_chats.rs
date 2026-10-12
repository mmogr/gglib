//! `/v1/chats` on the real proxy: a paired device lists and opens the hub's
//! chats, changes one (ADR 0017) and keeps a picture on one (ADR 0018), and
//! nothing else reaches them.
//!
//! The chats are a stand-in (`fixtures::chats`) that counts its calls,
//! because what is under test is the door: who passes, the bodies and the
//! error shape. What the list holds is the port's, tested where it lives.
//!
//! The door is one layer over one router group, which holds `/v1/attachments`
//! too, so the three tests of who passes run over all six routes here and
//! nowhere else. What an upload or a fetch answers is in
//! `integration_attachments.rs`.

use std::sync::Arc;

use gglib_core::domain::AttachmentId;
use gglib_core::ports::HubChatsPort;
use reqwest::{Client, RequestBuilder, StatusCode};

mod fixtures;
use fixtures::chats::{
    BRANCH_ID, FakeChats, NO_MESSAGE, OPEN_ID, TOOL_ROW, drawn, listed, opened, png, serve,
};
use fixtures::remote::from_device;
use fixtures::runs::{code, json};
use fixtures::tunnel::{DEVICE, DEVICE_KEY, PROXY_KEY, get, spawn_proxy_holding, tunnel_to};

/// A change to chat `id`, as a device sends one.
fn change(base: &str, id: &str, body: &serde_json::Value) -> RequestBuilder {
    Client::new()
        .post(format!("{base}/v1/chats/{id}/changes"))
        .json(body)
}

/// A regenerate of the reply `message`.
fn regenerate(message: i64) -> serde_json::Value {
    serde_json::json!({ "kind": "regenerate", "message_id": message })
}

/// The chat routes, as a client reaches them.
fn chat_routes(base: &str) -> Vec<RequestBuilder> {
    let client = Client::new();
    vec![
        client.get(format!("{base}/v1/chats")),
        client.get(format!("{base}/v1/chats/{OPEN_ID}")),
        change(base, &OPEN_ID.to_string(), &regenerate(2)),
    ]
}

/// An image, as a client sends one.
fn upload(base: &str) -> RequestBuilder {
    Client::new()
        .post(format!("{base}/v1/attachments"))
        .body(png(640, 480, 0))
}

/// A picture to keep on chat `id`, as a device sends one.
fn keep(base: &str, id: &str, body: &serde_json::Value) -> RequestBuilder {
    Client::new()
        .post(format!("{base}/v1/chats/{id}/pictures"))
        .json(body)
}

/// The body that keeps `image` beside `from`.
fn picture(from: &AttachmentId, image: &AttachmentId) -> serde_json::Value {
    serde_json::json!({ "from": from.as_str(), "image": image.as_str() })
}

/// Send `bytes` as the device and answer the id the hub stored them under.
async fn uploaded(base: &str, bytes: Vec<u8>) -> AttachmentId {
    let request = Client::new().post(format!("{base}/v1/attachments"));
    let (status, body) = json(from_device(request.body(bytes)).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    AttachmentId::parse(body["id"].as_str().unwrap()).unwrap()
}

/// Every route behind the door: the chats, a picture to keep, an upload,
/// and a fetch of an id no image has.
fn routes(base: &str) -> Vec<RequestBuilder> {
    let unknown = AttachmentId::of(b"never uploaded");
    let mut routes = chat_routes(base);
    let open = OPEN_ID.to_string();
    routes.push(keep(base, &open, &picture(&drawn().id, &unknown)));
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

/// A change is the hub's to make: the device is answered the chat to show,
/// and the change reaches the hub as it was sent.
#[tokio::test]
async fn a_named_device_changes_a_chat_and_is_told_where_it_went() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let edit = serde_json::json!({ "kind": "edit", "message_id": 1, "content": "why not?" });

    let request = from_device(change(&base, &OPEN_ID.to_string(), &edit));
    let (status, body) = json(request.send().await.unwrap()).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let told = serde_json::json!({ "conversation_id": BRANCH_ID, "forked": true, "answer": true });
    assert_eq!(body, told);
    let made = chats.changes.lock().unwrap().clone();
    assert_eq!(
        serde_json::to_value(made).unwrap(),
        serde_json::json!([edit])
    );
    cancel.cancel();
}

/// A change the rules refuse says why by its code, a chat that is not there
/// is not found, and a body that is no change is refused before the hub is
/// asked. None echoes what was sent.
#[tokio::test]
async fn a_change_that_cannot_be_made_says_why_and_echoes_nothing() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let open = OPEN_ID.to_string();
    let secret = serde_json::json!({ "kind": "edit", "message_id": 1, "content": "zzq-private" });
    let cases = [
        (
            change(&base, &open, &regenerate(NO_MESSAGE)),
            StatusCode::NOT_FOUND,
            "message_not_found",
        ),
        (
            change(&base, "12345", &regenerate(2)),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            change(
                &base,
                &open,
                &serde_json::json!({ "kind": "rewrite", "message_id": 2 }),
            ),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            change(
                &base,
                &open,
                &serde_json::json!({ "replace_from": 2, "content": "zzq-private" }),
            ),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            change(&base, "zzq-private", &secret),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
    ];
    for (request, want, code_want) in cases {
        let (status, body) = json(from_device(request).send().await.unwrap()).await;
        assert_eq!((status, code(&body)), (want, code_want), "{body}");
        assert_eq!(body["error"]["type"], "invalid_request_error", "{body}");
        assert!(!body.to_string().contains("zzq-private"), "{body}");
    }
    assert!(chats.changes.lock().unwrap().is_empty());
    cancel.cancel();
}

/// A picture kept beside the one a tool drew is answered with its row's
/// images, itself last, and the chat then opens with it there. Kept again
/// from the first, a second is last.
#[tokio::test]
async fn a_named_device_keeps_a_picture_and_reads_it_back_last_on_its_tool_row() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let open = OPEN_ID.to_string();
    let fixed = uploaded(&base, png(1024, 768, 0)).await;
    let again = uploaded(&base, png(512, 512, 0)).await;

    let request = from_device(keep(&base, &open, &picture(&drawn().id, &fixed)));
    let (status, body) = json(request.send().await.unwrap()).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let want = serde_json::json!([
        serde_json::to_value(drawn()).unwrap(),
        { "id": fixed.as_str(), "mime": "image/png", "width": 1024, "height": 768 },
    ]);
    assert_eq!(body, want);

    let request = from_device(keep(&base, &open, &picture(&fixed, &again)));
    let (status, body) = json(request.send().await.unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().unwrap().len(), 3, "{body}");

    let request = from_device(Client::new().get(format!("{base}/v1/chats/{open}")));
    let (_, read) = json(request.send().await.unwrap()).await;
    let row = &read["messages"][1];
    assert_eq!(
        (&row["id"], &row["role"]),
        (&TOOL_ROW.into(), &"tool".into())
    );
    let ids: Vec<&str> = row["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|image| image["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [drawn().id.as_str(), fixed.as_str(), again.as_str()]);
    cancel.cancel();
}

/// A picture that cannot be kept says why by its code: a chat that is not
/// there is not found, an id the body names that is not there is a 400, and
/// a body that is not the two ids is refused before the hub is asked. None
/// echoes what was sent, and the row is as it was.
#[tokio::test]
async fn a_picture_that_cannot_be_kept_says_why_and_echoes_nothing() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let open = OPEN_ID.to_string();
    let fixed = uploaded(&base, png(1024, 768, 0)).await;
    // Stored, and on no tool row of the chat.
    let stray = uploaded(&base, png(320, 240, 0)).await;
    let unstored = AttachmentId::of(b"never uploaded");
    let good = picture(&drawn().id, &fixed);
    let asked = chats.calls();
    let not_asked = [
        (
            keep(&base, "zzq-private", &good),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            keep(
                &base,
                &open,
                &serde_json::json!({
                    "from": drawn().id.as_str(),
                    "image": fixed.as_str(),
                    "note": "zzq-private",
                }),
            ),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            keep(
                &base,
                &open,
                &serde_json::json!({ "from": drawn().id.as_str() }),
            ),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            keep(
                &base,
                &open,
                &serde_json::json!({ "from": "zzq-private", "image": fixed.as_str() }),
            ),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            keep(&base, &open, &good).body("zzq-private"),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
    ];
    let asked_and_refused = [
        (
            keep(&base, "12345", &good),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            keep(&base, &open, &picture(&stray, &fixed)),
            StatusCode::BAD_REQUEST,
            "attachment_not_found",
        ),
        (
            keep(&base, &open, &picture(&drawn().id, &unstored)),
            StatusCode::BAD_REQUEST,
            "attachment_not_found",
        ),
    ];
    for (cases, reaches_the_hub) in [(Vec::from(not_asked), 0), (Vec::from(asked_and_refused), 3)] {
        for (request, want, code_want) in cases {
            let (status, body) = json(from_device(request).send().await.unwrap()).await;
            assert_eq!((status, code(&body)), (want, code_want), "{body}");
            assert_eq!(body["error"]["type"], "invalid_request_error", "{body}");
            assert!(body["error"]["message"].is_string(), "{body}");
            for sent in ["zzq-private", stray.as_str(), unstored.as_str()] {
                assert!(!body.to_string().contains(sent), "{body}");
            }
        }
        assert_eq!(chats.calls() - asked, reaches_the_hub);
    }

    let request = from_device(Client::new().get(format!("{base}/v1/chats/{open}")));
    let (_, read) = json(request.send().await.unwrap()).await;
    assert_eq!(read, serde_json::to_value(opened()).unwrap());
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
    let (status, stored) = json(sent.send().await.unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{stored}");
    // With the key and the device, the picture `routes` could not keep is
    // kept once its image is stored.
    let body = serde_json::json!({ "from": drawn().id.as_str(), "image": stored["id"] });
    let kept = from_device(keep(&base, &OPEN_ID.to_string(), &body)).bearer_auth("secret123");
    assert_eq!(kept.send().await.unwrap().status(), StatusCode::OK);
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
