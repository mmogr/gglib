//! A render asked of the far machine: what its proxy is sent, and that the
//! stream it answers with is handed back as it came.

use gglib_core::contracts::http::images::ImageGenerationsRequest;
use gglib_test_http::Request;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::super::FarProxy;
use super::super::far_proxy_tests::credentials;

/// The events the stand-in far proxy answers with.
const EVENTS: &str = "data: {\"type\":\"image_generation.progress\",\"stage\":\"loading\"}\n\n";

/// A far proxy on a loopback port that reads one request, hands it back,
/// and answers it with [`EVENTS`].
async fn far_proxy() -> (String, oneshot::Receiver<Request>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (seen, heard) = oneshot::channel();
    tokio::spawn(async move {
        let (request, mut socket) = gglib_test_http::accept(&listener).await.unwrap();
        let _ = seen.send(request);
        let answer = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{EVENTS}",
            EVENTS.len()
        );
        let _ = socket.write_all(answer.as_bytes()).await;
    });
    (format!("http://127.0.0.1:{port}/v1"), heard)
}

#[tokio::test]
async fn a_render_is_posted_to_that_machine_with_its_key_and_read_back_as_it_came() {
    let (base, heard) = far_proxy().await;
    let far = FarProxy::new(&base, &credentials()).unwrap();
    let asked = ImageGenerationsRequest {
        model: Some("11".to_owned()),
        prompt: "a happy kitty".to_owned(),
        stream: true,
        ..ImageGenerationsRequest::default()
    };

    let body = far
        .generate_images(&asked)
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(body, EVENTS);
    let request = heard.await.unwrap();
    assert_eq!(request.line(), "POST /v1/images/generations HTTP/1.1");
    assert_eq!(request.header("authorization"), Some("Bearer sk-the-key"));
    assert_eq!(request.header("accept"), Some("text/event-stream"));
    let sent: ImageGenerationsRequest = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(sent.model.as_deref(), Some("11"));
    assert_eq!(sent.prompt, "a happy kitty");
    assert!(sent.stream);
}

/// A model left out is left out on the wire, so that machine draws with
/// its own default and not with one this machine chose.
#[tokio::test]
async fn no_model_is_sent_when_none_was_named() {
    let (base, heard) = far_proxy().await;
    let far = FarProxy::new(&base, &credentials()).unwrap();
    let asked = ImageGenerationsRequest {
        prompt: "a happy kitty".to_owned(),
        stream: true,
        ..ImageGenerationsRequest::default()
    };

    far.generate_images(&asked).await.unwrap();

    let body: serde_json::Value = serde_json::from_slice(&heard.await.unwrap().body).unwrap();
    assert!(
        body.get("model").is_none_or(serde_json::Value::is_null),
        "{body}"
    );
}

#[test]
fn a_refused_key_names_the_machine_and_says_how_to_pair_again() {
    let far = FarProxy::new("http://127.0.0.1:9/v1", &credentials()).unwrap();
    let text = far.key_refused();
    assert!(text.starts_with(far.shown_name()), "{text}");
    assert!(text.contains("gglib remote invite"), "{text}");
}
