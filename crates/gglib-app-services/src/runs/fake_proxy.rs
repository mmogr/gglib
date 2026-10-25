//! A loopback listener that plays the proxy for one request.
//!
//! It reads the whole request (head, then the body its length names) before
//! it writes a byte, so the client never meets a reset for a request it was
//! still sending. The test then writes the answer in parts and closes it;
//! a client that leaves first is reported.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::runs::RunError;
use serde_json::Value;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

use super::door::{Door, ProxyDoor};

/// The head of a streamed 200.
pub(super) const STREAM_HEAD: &str =
    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";

/// The request the fake received.
pub(super) struct Seen {
    /// The request line and headers, as sent.
    pub(super) head: String,
    pub(super) body: Value,
}

pub(super) struct FakeProxy {
    pub(super) addr: SocketAddr,
    parts: mpsc::UnboundedSender<Option<Vec<u8>>>,
    pub(super) seen: oneshot::Receiver<Seen>,
    /// Fires when the client closes the connection before the fake does.
    pub(super) client_left: oneshot::Receiver<()>,
}

impl FakeProxy {
    pub(super) async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (parts, mut rx) = mpsc::unbounded_channel::<Option<Vec<u8>>>();
        let (seen_tx, seen) = oneshot::channel();
        let (left_tx, client_left) = oneshot::channel();
        tokio::spawn(async move {
            let (request, mut socket) = gglib_test_http::accept(&listener)
                .await
                .expect("the whole request");
            let body = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let _ = seen_tx.send(Seen {
                head: request.head,
                body,
            });
            let mut chunk = [0_u8; 4096];
            loop {
                tokio::select! {
                    part = rx.recv() => match part {
                        Some(Some(bytes)) => {
                            if socket.write_all(&bytes).await.is_err() {
                                let _ = left_tx.send(());
                                return;
                            }
                        }
                        _ => return,
                    },
                    n = socket.read(&mut chunk) => {
                        if matches!(n, Ok(0) | Err(_)) {
                            let _ = left_tx.send(());
                            return;
                        }
                    }
                }
            }
        });
        Self {
            addr,
            parts,
            seen,
            client_left,
        }
    }

    /// Write `bytes` to the client.
    pub(super) fn say(&self, bytes: impl Into<Vec<u8>>) {
        let _ = self.parts.send(Some(bytes.into()));
    }

    /// Close the connection.
    pub(super) fn close(&self) {
        let _ = self.parts.send(None);
    }
}

/// A door that always opens on the same address with the same key, or
/// never opens.
pub(super) struct FixedDoor(pub(super) Option<(SocketAddr, Option<String>)>);

#[async_trait]
impl ProxyDoor for FixedDoor {
    async fn open(&self) -> Result<Option<Door>, RunError> {
        Ok(self.0.clone().map(|(addr, key)| Door { addr, key }))
    }
}

/// A door on `addr` presenting `key`.
pub(super) fn door(addr: SocketAddr, key: Option<&str>) -> Arc<FixedDoor> {
    Arc::new(FixedDoor(Some((addr, key.map(str::to_owned)))))
}
