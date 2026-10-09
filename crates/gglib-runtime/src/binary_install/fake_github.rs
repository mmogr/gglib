//! A loopback stand-in for GitHub's release API and download host, so the
//! install pipeline runs end to end in a test with no network.
//!
//! It reads each request's head before it answers: a server that answers and
//! drops the socket unread can reset it, and the client then loses the reply.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One canned answer: a status line's code and the body.
#[derive(Clone)]
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
}

impl Reply {
    pub(crate) fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
        }
    }

    pub(crate) fn not_found() -> Self {
        Self {
            status: 404,
            body: br#"{"message":"Not Found"}"#.to_vec(),
        }
    }
}

/// The fake, serving its routes by request path; any other path is a 404.
pub(crate) struct FakeGitHub {
    /// `http://127.0.0.1:<port>`, the API base and the download host both.
    pub(crate) base: String,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl FakeGitHub {
    /// Listen on a loopback port, with no routes yet.
    pub(crate) async fn serve() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let routes: Arc<Mutex<HashMap<String, Reply>>> = Arc::default();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let (table, log) = (Arc::clone(&routes), Arc::clone(&asked));
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let (table, log) = (Arc::clone(&table), Arc::clone(&log));
                tokio::spawn(async move {
                    let Some(path) = read_head(&mut socket).await else {
                        return;
                    };
                    log.lock().expect("the log").push(path.clone());
                    let reply = table
                        .lock()
                        .expect("the routes")
                        .get(&path)
                        .cloned()
                        .unwrap_or_else(Reply::not_found);
                    let head = format!(
                        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        reply.status,
                        reply.body.len()
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(&reply.body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self {
            base,
            routes,
            asked,
        }
    }

    /// Answer `path` with `reply` from now on.
    pub(crate) fn route(&self, path: &str, reply: Reply) {
        self.routes
            .lock()
            .expect("the routes")
            .insert(path.to_owned(), reply);
    }

    /// Every path asked for, in order.
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("the log").clone()
    }
}

/// Read a request head to its blank line and return the path it asks for.
async fn read_head(socket: &mut tokio::net::TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if socket.read(&mut byte).await.ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head);
    text.split_whitespace().nth(1).map(str::to_owned)
}
