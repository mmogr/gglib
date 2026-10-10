//! A loopback stand-in for `sd-server`, for tests: [`FakeSdServer`].
//!
//! It answers `GET /v1/models` the way sd-server does, and holds every render
//! request (`POST /sdcpp/v1/img_gen`, `POST /v1/images/generations`) open
//! without answering, as a real render holds the server for a minute or more.
//! Each connection is its own task, so a held render never delays a probe:
//! that is the shape of sd-server's thread pool, where `/v1/models` takes no
//! lock and a render takes `sd_ctx_mutex`.
//!
//! It reads each request's head before it answers: a server that answers and
//! drops the socket unread can reset it, and the client then loses the reply.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What sd-server 228c707's `/v1/models` answers, whatever it loaded.
pub(crate) const SD_MODELS_BODY: &str =
    r#"{"object":"list","data":[{"id":"sd-cpp-local","object":"model","owned_by":"local"}]}"#;

/// The fake, listening on a loopback port.
pub(crate) struct FakeSdServer {
    /// The port it listens on.
    pub(crate) port: u16,
    held: Arc<AtomicUsize>,
}

impl FakeSdServer {
    /// An sd-server: `/v1/models` lists `sd-cpp-local`.
    pub(crate) async fn serve() -> Self {
        Self::serve_models_body(SD_MODELS_BODY).await
    }

    /// A server that answers `/v1/models` with `body` and is otherwise the
    /// same; given another body it stands for a foreign server on the port.
    pub(crate) async fn serve_models_body(body: &'static str) -> Self {
        Self::listen(TcpListener::bind("127.0.0.1:0").await.expect("bind"), body)
    }

    /// An sd-server on `port`: for a launch test whose stand-in binary only
    /// records the port it was given, so the fake answers there in its place.
    /// Unix only, as that launch test is.
    #[cfg(unix)]
    pub(crate) async fn serve_on(port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind the launched port");
        Self::listen(listener, SD_MODELS_BODY)
    }

    fn listen(listener: TcpListener, body: &'static str) -> Self {
        let port = listener.local_addr().expect("an address").port();
        let held = Arc::new(AtomicUsize::new(0));
        let renders = Arc::clone(&held);
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(answer(socket, body, Arc::clone(&renders)));
            }
        });
        Self { port, held }
    }

    /// How many render requests it is holding open now.
    pub(crate) fn renders_held(&self) -> usize {
        self.held.load(Ordering::SeqCst)
    }
}

async fn answer(mut socket: TcpStream, models_body: &'static str, renders: Arc<AtomicUsize>) {
    let Some((method, path)) = read_head(&mut socket).await else {
        return;
    };
    match (method.as_str(), path.as_str()) {
        ("GET", "/v1/models") => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                models_body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(models_body.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
        ("POST", "/sdcpp/v1/img_gen" | "/v1/images/generations") => {
            // A render: hold the socket open and never answer. The count
            // falls when the client gives up and the read sees the close.
            renders.fetch_add(1, Ordering::SeqCst);
            let mut sink = [0u8; 1024];
            while socket.read(&mut sink).await.is_ok_and(|n| n > 0) {}
            renders.fetch_sub(1, Ordering::SeqCst);
        }
        _ => {
            let body = r#"{"error":"not found"}"#;
            let head = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(body.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    }
}

/// Read a request head to its blank line; its method and path.
async fn read_head(socket: &mut TcpStream) -> Option<(String, String)> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if socket.read(&mut byte).await.ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head);
    let mut words = text.split_whitespace();
    Some((words.next()?.to_owned(), words.next()?.to_owned()))
}
