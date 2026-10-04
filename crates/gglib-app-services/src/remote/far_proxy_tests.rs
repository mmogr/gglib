//! What `FarProxy` refuses before anything is sent, what it never shows, and
//! how long a stream may run.

use std::time::Duration;

use gglib_core::domain::hub_chats::HubTurn;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::FarProxy;
use crate::error::GuiError;
use crate::remote::paired_machine::FarCredentials;

/// Nothing listens here: a request that got as far as sending would fail as
/// `Unavailable`, so a `ValidationFailed` proves it was never sent.
const NOWHERE: &str = "http://127.0.0.1:9/v1";

fn credentials() -> FarCredentials {
    FarCredentials {
        key: "sk-the-key".to_owned(),
        fingerprint: "0a1b2c3d4e5f".to_owned(),
        name: None,
    }
}

#[tokio::test]
async fn a_run_id_that_could_name_another_route_is_refused_before_sending() {
    let far = FarProxy::new(NOWHERE, &credentials()).unwrap();
    let turn = HubTurn {
        conversation_id: 1,
        content: "hi".to_owned(),
        images: Vec::new(),
    };
    for id in ["..", "a/b", "", "run.1", &"x".repeat(65)] {
        assert!(
            matches!(
                far.add_turn(id, &turn).await,
                Err(GuiError::ValidationFailed(_))
            ),
            "{id:?}"
        );
        assert!(
            matches!(far.cancel_run(id).await, Err(GuiError::ValidationFailed(_))),
            "{id:?}"
        );
        assert!(
            matches!(
                far.run_events(id, 0).await,
                Err(GuiError::ValidationFailed(_))
            ),
            "{id:?}"
        );
    }
}

#[test]
fn its_debug_form_never_prints_the_key() {
    let far = FarProxy::new(NOWHERE, &credentials()).unwrap();
    let shown = format!("{far:?}");
    assert!(!shown.contains("sk-the-key"), "{shown}");
    assert!(shown.contains(NOWHERE), "{shown}");
}

/// It is always the paired machine it was built for, shown by a name and
/// never by its fingerprint, and the completion adapter is pointed at its
/// root, where it adds the `/v1` itself.
#[test]
fn it_names_its_machine_and_its_root() {
    let far = FarProxy::new(NOWHERE, &credentials()).unwrap();
    assert_eq!(
        far.machine(),
        gglib_core::domain::Machine::Paired {
            fingerprint: "0a1b2c3d4e5f".to_owned()
        }
    );
    assert_eq!(far.server_root(), "http://127.0.0.1:9");
    assert_eq!(far.shown_name(), gglib_core::domain::UNNAMED_PAIRED);
    assert_eq!(far.far_machine().name, gglib_core::domain::UNNAMED_PAIRED);
}

/// A model picked from one paired machine's list is sent to that machine
/// alone: once the pairing names another, its same id is another model.
#[test]
fn it_serves_only_the_machine_it_is_on() {
    use gglib_core::domain::Machine;
    let paired = |fingerprint: &str| Machine::Paired {
        fingerprint: fingerprint.to_owned(),
    };
    let far = || FarProxy::new(NOWHERE, &credentials()).unwrap();

    assert!(far().serving(&paired("0a1b2c3d4e5f")).is_ok());
    for other in [paired("ffeeddccbbaa"), Machine::Local] {
        let Err(GuiError::Conflict(message)) = far().serving(&other) else {
            panic!("{other:?} was served by another machine's proxy");
        };
        assert!(
            message.contains("changed since this model was picked"),
            "{message}"
        );
        assert!(!message.contains("0a1b2c3d4e5f"), "{message}");
    }
}

/// A far proxy on a loopback port that reads the request head, then answers
/// a chunked event stream: `frames` frames `gap` apart, then silence for
/// `hold` before the end.
async fn stream_server(frames: usize, gap: Duration, hold: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).await.unwrap();
            head.push(byte[0]);
        }
        let start = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                     transfer-encoding: chunked\r\n\r\n";
        socket.write_all(start.as_bytes()).await.unwrap();
        for n in 0..frames {
            let frame = format!("id: {n}\ndata: {{}}\n\n");
            let chunk = format!("{:x}\r\n{frame}\r\n", frame.len());
            if socket.write_all(chunk.as_bytes()).await.is_err() {
                return;
            }
            tokio::time::sleep(gap).await;
        }
        tokio::time::sleep(hold).await;
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });
    format!("http://127.0.0.1:{port}/v1")
}

fn far_reading_with(base_url: &str, read_timeout: Duration) -> FarProxy {
    let bounded = super::build(gglib_proxy::loopback::client_builder()).unwrap();
    let streaming = super::build(super::streaming_builder(read_timeout)).unwrap();
    FarProxy::with_clients(base_url, &credentials(), bounded, streaming)
}

/// A stream has no limit end to end: one that keeps sending lasts well past
/// its read timeout and arrives whole.
#[tokio::test]
async fn a_stream_that_keeps_sending_outlasts_its_read_timeout() {
    let base = stream_server(8, Duration::from_millis(100), Duration::ZERO).await;
    let far = far_reading_with(&base, Duration::from_millis(300));

    let body = far
        .run_events("chat-1", 0)
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(body.matches("data:").count(), 8, "{body}");
}

/// A stream that goes silent is ended by the read timeout, not waited on.
#[tokio::test]
async fn a_silent_stream_is_ended_by_its_read_timeout() {
    let base = stream_server(1, Duration::ZERO, Duration::from_secs(10)).await;
    let far = far_reading_with(&base, Duration::from_millis(300));

    let read = tokio::time::timeout(Duration::from_secs(5), async {
        far.run_events("chat-1", 0).await.unwrap().text().await
    })
    .await
    .expect("the silence ended the read, not the test");

    assert!(read.is_err(), "{read:?}");
}

/// The shipped limits: none end to end, and silence of three of the far
/// proxy's 15-second keep-alives.
#[test]
fn a_stream_is_bounded_on_silence_only() {
    assert_eq!(super::STREAM_TOTAL_TIMEOUT, None);
    assert_eq!(super::STREAM_READ_TIMEOUT, Duration::from_secs(45));
}

/// A far machine that is not there is a fixed sentence, not the client's.
#[tokio::test]
async fn a_far_machine_that_does_not_answer_is_said_in_fixed_words() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    };
    let far = FarProxy::new(&format!("http://127.0.0.1:{port}/v1"), &credentials()).unwrap();

    let err = far.list_chats().await.unwrap_err();

    assert!(
        matches!(&err, GuiError::Unavailable(m) if m == super::NO_ANSWER),
        "{err:?}"
    );
}
