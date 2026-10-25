//! A stand-in for llama-server on a loopback port, for a test that runs the
//! built `gglib` against `--port`: it answers every request with one short
//! streamed reply and hands over what each completion request carried.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, channel};

use gglib_test_http::blocking;

/// One short reply, as llama-server streams it.
const REPLY: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\
     \"finish_reason\":null}]}\n\n\
     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
     data: [DONE]\n\n";

/// Start the stand-in: its port, and each completion request's body as it
/// arrives. Every request is read whole and answered with [`REPLY`].
pub(crate) fn model_server() -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let (received, completions) = channel();
    std::thread::spawn(move || {
        loop {
            let Ok((request, mut stream)) = blocking::accept(&listener) else {
                continue;
            };
            if request.line().starts_with("POST /v1/chat/completions") {
                let _ = received.send(request.body_text());
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{REPLY}",
                REPLY.len()
            );
        }
    });
    (port, completions)
}
