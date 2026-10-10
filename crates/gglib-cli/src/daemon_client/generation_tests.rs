//! `gglib chat`'s side of the daemon's generation gate: the route's events
//! read, a turn that is its connection, and no daemon meaning no wait.

use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::*;
use crate::daemon_client::STAND_IN_PORT;
use crate::daemon_client::handle_tests::nobody;

fn event(name: &str, data: &str) -> Event {
    let text = format!("event: {name}\ndata: {data}\n\n");
    let mut events = DataFrames::unbounded().push_events(text.as_bytes());
    assert_eq!(events.len(), 1);
    events.remove(0)
}

fn handle() -> DaemonHandle {
    DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    }
}

/// Each event the route sends is read as what it says; one this CLI does
/// not know is passed over.
#[test]
fn the_routes_events_are_read() {
    assert_eq!(
        signal(&event("waiting", r#"{"step":3,"total":20,"position":2}"#)),
        Some(Signal::Waiting(GateWait {
            reason: WaitReason::ImageRender,
            step: 3,
            total: 20,
            position: 2,
        }))
    );
    assert_eq!(signal(&event("granted", "{}")), Some(Signal::Granted));
    assert_eq!(
        signal(&event(
            "refused",
            r#"{"message":"waited 180s","stalled_secs":180}"#
        )),
        Some(Signal::Refused(GateError::Stalled(Duration::from_mins(3))))
    );
    assert_eq!(
        signal(&event("refused", r#"{"message":"no","stalled_secs":null}"#)),
        Some(Signal::Refused(GateError::Unavailable("no".to_owned())))
    );
    assert_eq!(signal(&event("later", "{}")), None);
}

/// A daemon stand-in that answers the route with a wait and a grant, then
/// reports when the connection closes.
async fn granting() -> (u16, oneshot::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (closed, gone) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            assert_eq!(socket.read(&mut byte).await.unwrap(), 1, "the head ended");
            head.push(byte[0]);
        }
        assert!(head.starts_with(b"GET /api/generation/turn "));
        let reply = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n\
                     event: waiting\ndata: {\"step\":1,\"total\":4,\"position\":1}\n\n\
                     event: granted\ndata: {}\n\n";
        socket.write_all(reply.as_bytes()).await.unwrap();
        // Read until the client closes its end.
        let mut rest = [0_u8; 64];
        while socket.read(&mut rest).await.is_ok_and(|n| n > 0) {}
        let _ = closed.send(());
    });
    (port, gone)
}

/// Keeps the waits it is told.
#[derive(Debug, Default)]
struct Told(Mutex<Vec<GateWait>>);

impl GateWaitObserver for Told {
    fn waiting(&self, wait: GateWait) {
        self.0.lock().unwrap().push(wait);
    }
}

/// The turn is granted after the wait is told, holds its connection open,
/// and closes it when the turn ends.
#[tokio::test]
async fn the_turn_is_its_connection() {
    let (port, mut gone) = granting().await;
    let gate = STAND_IN_PORT.scope(port, async { DaemonGenerationGate::new(&handle(), true) });
    let gate = gate.await;
    let told = Arc::new(Told::default());

    let turn = gate
        .llm_turn(Some(Arc::clone(&told) as Arc<dyn GateWaitObserver>))
        .await
        .expect("granted");
    assert_eq!(told.0.lock().unwrap().len(), 1);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(gone.try_recv().is_err(), "the connection closed while held");

    drop(turn);
    let closed = tokio::time::timeout(Duration::from_secs(5), gone).await;
    assert!(closed.is_ok(), "the connection outlived its turn");
}

/// With no daemon to ask, a turn is granted at once and holds nothing, and
/// the session is told once.
#[tokio::test]
async fn with_no_daemon_a_turn_holds_nothing() {
    let gate = STAND_IN_PORT.scope(nobody(), async {
        DaemonGenerationGate::new(&handle(), true)
    });
    let gate = gate.await;
    for _ in 0..2 {
        let turn = tokio::time::timeout(Duration::from_secs(5), gate.llm_turn(None)).await;
        assert!(turn.expect("at once").is_ok());
    }
    assert!(gate.warned.load(Ordering::Relaxed));
}

/// This CLI grants no render turn: it draws through the daemon.
#[tokio::test]
async fn no_render_turn_is_granted_here() {
    #[derive(Debug)]
    struct Nothing;
    impl gglib_core::ports::AdmissionRelease for Nothing {
        fn release(&self, _slot: usize) {}
        fn progress(&self, _slot: usize) {}
    }
    let gate = STAND_IN_PORT.scope(nobody(), async {
        DaemonGenerationGate::new(&handle(), true)
    });
    let lease = AdmissionLease::new(Arc::new(Nothing), 0);
    let refused = gate.await.render_turn(lease, None).await;
    assert!(matches!(refused, Err(GateError::Unavailable(_))));
}
