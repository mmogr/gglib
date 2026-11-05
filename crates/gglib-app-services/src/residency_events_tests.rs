//! What the daemon's residency observer sends on `/api/events`: the frames
//! as a client reads them, key order included, and the health watch that
//! runs from a load to its unload.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gglib_core::domain::RuntimeKind;
use tokio::sync::Notify;

use super::*;
use crate::test_support::RecordingEmitter;

fn events() -> (ResidencyEvents, Arc<RecordingEmitter>) {
    let emitter = Arc::new(RecordingEmitter::default());
    let events = ResidencyEvents::new(Arc::clone(&emitter) as Arc<dyn AppEventEmitter>);
    (events, emitter)
}

/// The handle a launch announces: no pid, so the watch asks only HTTP.
fn handle(model_id: i64, name: &str, port: u16) -> ProcessHandle {
    ProcessHandle::new(model_id, name.to_owned(), None, port, 0)
}

/// The lifecycle frames sent so far. Health changes are left out: the
/// watch reports on its own clock.
fn frames(emitter: &RecordingEmitter) -> Vec<String> {
    emitter
        .events()
        .iter()
        .filter(|event| !matches!(event, AppEvent::ServerHealthChanged { .. }))
        .map(|event| serde_json::to_string(event).expect("an event serializes"))
        .collect()
}

#[tokio::test]
async fn a_loaded_model_is_announced_started_with_its_id_name_and_port() {
    let (events, emitter) = events();

    events.loaded(handle(42, "test-model", 9001));

    assert_eq!(
        frames(&emitter),
        [r#"{"type":"server_started","modelId":42,"modelName":"test-model","port":9001}"#]
    );
    assert_eq!(events.watched(), [42]);
}

#[tokio::test]
async fn an_unloaded_model_is_announced_stopped_by_the_name_it_was_loaded_with() {
    let (events, emitter) = events();
    events.loaded(handle(42, "test-model", 9001));

    events.unloaded(42);

    assert_eq!(
        frames(&emitter),
        [
            r#"{"type":"server_started","modelId":42,"modelName":"test-model","port":9001}"#,
            r#"{"type":"server_stopped","modelId":42,"modelName":"test-model"}"#,
        ]
    );
    assert_eq!(events.watched(), Vec::<u32>::new());
}

/// Every unload follows a load; one that does not is dropped rather than
/// announced with no name.
#[tokio::test]
async fn a_model_never_announced_loaded_is_not_announced_stopped() {
    let (events, emitter) = events();

    events.unloaded(7);

    assert_eq!(frames(&emitter), Vec::<String>::new());
}

/// Loaded again with no unload between: the earlier watch is ended, so
/// one model is never reported on by two.
#[tokio::test]
async fn a_model_loaded_twice_keeps_one_watch_and_ends_the_other() {
    let (events, _emitter) = events();
    events.loaded(handle(42, "test-model", 9001));
    let first = events.watch_token(42).expect("watched");

    events.loaded(handle(42, "test-model", 9002));

    assert_eq!(events.watched(), [42]);
    assert!(first.is_cancelled(), "the first watch was ended");
    assert!(!events.watch_token(42).unwrap().is_cancelled());
}

/// A stand-in `sd-server` on a loopback port: `/v1/models` answers with
/// sd-server's model, and anything else, `/health` included, is a 404. Each
/// request is read whole before the answer.
async fn fake_sd_server() -> u16 {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok(incoming) = gglib_test_http::incoming(&listener).await {
            tokio::spawn(async move {
                let Ok((request, mut socket)) = incoming.request().await else {
                    return;
                };
                let body = r#"{"object":"list","data":[{"id":"sd-cpp-local","object":"model"}]}"#;
                let reply = if request.line().starts_with("GET /v1/models ") {
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else {
                    "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                        .to_owned()
                };
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    port
}

/// A stand-in `sd-server` that holds every request open until `release` is
/// notified, counting each in `arrived`, then answers `/v1/models` as
/// sd-server does.
async fn held_sd_server() -> (u16, Arc<Notify>, Arc<AtomicUsize>) {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let release = Arc::new(Notify::new());
    let arrived = Arc::new(AtomicUsize::new(0));
    let (task_release, task_arrived) = (Arc::clone(&release), Arc::clone(&arrived));
    tokio::spawn(async move {
        while let Ok(incoming) = gglib_test_http::incoming(&listener).await {
            let (release, arrived) = (Arc::clone(&task_release), Arc::clone(&task_arrived));
            tokio::spawn(async move {
                let Ok((_request, mut socket)) = incoming.request().await else {
                    return;
                };
                arrived.fetch_add(1, Ordering::SeqCst);
                release.notified().await;
                let body = r#"{"object":"list","data":[{"id":"sd-cpp-local","object":"model"}]}"#;
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    (port, release, arrived)
}

/// Wait up to five seconds for `condition`.
async fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    condition()
}

/// A check that was in flight when the model was unloaded says nothing: no
/// `server_health_changed` follows `server_stopped`.
#[tokio::test]
async fn a_check_in_flight_at_the_unload_sends_nothing_after_server_stopped() {
    let emitter = Arc::new(RecordingEmitter::default());
    let events = ResidencyEvents::with_check_interval(
        Arc::clone(&emitter) as Arc<dyn AppEventEmitter>,
        Duration::from_millis(20),
    );
    let (port, release, arrived) = held_sd_server().await;

    events.loaded(handle(42, "flux", port).with_runtime(RuntimeKind::StableDiffusion));
    assert!(
        eventually(|| arrived.load(Ordering::SeqCst) >= 1).await,
        "the first check reached the server"
    );
    events.unloaded(42);
    release.notify_one();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let sent = emitter.events();
    let stopped_at = sent
        .iter()
        .position(|e| matches!(e, AppEvent::ServerStopped { .. }))
        .expect("server_stopped was sent");
    assert!(
        !sent[stopped_at..]
            .iter()
            .any(|e| matches!(e, AppEvent::ServerHealthChanged { .. })),
        "{sent:?}"
    );
    assert_eq!(arrived.load(Ordering::SeqCst), 1, "the watch asked no more");
}

/// The watch asks a loaded server the way its runtime answers: an
/// `sd-server`, which has no `/health`, reads as healthy. The unload ends
/// the watch, and nothing follows `server_stopped`.
#[tokio::test]
async fn a_loaded_image_model_is_watched_as_sd_server_until_its_unload() {
    let (events, emitter) = events();
    let port = fake_sd_server().await;

    events.loaded(handle(42, "flux", port).with_runtime(RuntimeKind::StableDiffusion));
    let mut first = None;
    for _ in 0..100 {
        first = emitter.events().into_iter().find_map(|event| match event {
            AppEvent::ServerHealthChanged {
                server_id,
                model_id,
                status,
                ..
            } => Some((server_id, model_id, status)),
            _ => None,
        });
        if first.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    events.unloaded(42);

    assert_eq!(first, Some((1, 42, ServerHealthStatus::Healthy)));
    assert_eq!(events.watched(), Vec::<u32>::new());
    assert!(
        matches!(
            emitter.events().last(),
            Some(AppEvent::ServerStopped { model_id: 42, .. })
        ),
        "{:?}",
        emitter.events()
    );
}
