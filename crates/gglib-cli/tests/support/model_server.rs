//! A stand-in for llama-server on a loopback port, for a test that runs the
//! built `gglib` against `--port`: it answers every request with one short
//! streamed reply and hands over what each completion request carried.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, channel};

/// One short reply, as llama-server streams it.
const REPLY: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\
     \"finish_reason\":null}]}\n\n\
     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
     data: [DONE]\n\n";

/// Read one request whole, so the answer is not sent over unread bytes: its
/// request line and its body.
fn read_request(stream: &mut TcpStream) -> (String, String) {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0_u8; length];
    let _ = stream.read_exact(&mut body);
    let line = head.lines().next().unwrap_or_default().to_owned();
    (line, String::from_utf8_lossy(&body).into_owned())
}

/// Start the stand-in: its port, and each completion request's body as it
/// arrives. Every request is answered with [`REPLY`].
pub(crate) fn model_server() -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let (received, completions) = channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let (line, body) = read_request(&mut stream);
            if line.starts_with("POST /v1/chat/completions") {
                let _ = received.send(body);
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
