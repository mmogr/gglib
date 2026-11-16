//! A stand-in daemon for a test of what a command sends and prints: it
//! answers scripted replies in order and keeps what it was sent.

use std::io::Write as _;
use std::net::TcpListener;

use gglib_test_http::blocking;

use super::{DaemonHandle, STAND_IN_PORT};

/// One request the stand-in daemon took: its request line and its body.
pub(crate) struct Taken {
    pub(crate) line: String,
    pub(crate) body: serde_json::Value,
}

/// A stand-in daemon that answers `replies` in order, each `(status, body)`,
/// closing every connection, and `f` run against it. Answers what `f`
/// answered beside the requests the stand-in took, which may be fewer than
/// the replies scripted: once `f` is over, each reply still waiting is
/// woken with an empty connection and the stand-in stops, so a request
/// that never came is a short count and not a hang.
pub(crate) async fn against<T, F, Fut>(replies: Vec<(u16, String)>, f: F) -> (T, Vec<Taken>)
where
    F: FnOnce(DaemonHandle) -> Fut,
    Fut: Future<Output = T>,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let addr = listener.local_addr().expect("its address");
    let port = addr.port();
    let scripted = replies.len();
    let daemon = std::thread::spawn(move || {
        let mut taken = Vec::new();
        for (status, reply) in replies {
            // An empty connection is the wake-up, not a request.
            let Ok((request, mut stream)) = blocking::accept(&listener) else {
                break;
            };
            let reason = if status == 200 { "OK" } else { "Refused" };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
            taken.push(Taken {
                line: request.line().to_owned(),
                body: serde_json::from_str(&request.body_text()).unwrap_or_default(),
            });
        }
        taken
    });
    let handle = DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    };
    let answered = STAND_IN_PORT.scope(port, f(handle)).await;
    for _ in 0..scripted {
        // Wakes a reply still waiting; refused once the stand-in has stopped.
        drop(std::net::TcpStream::connect(addr));
    }
    (answered, daemon.join().expect("the stand-in ran"))
}
