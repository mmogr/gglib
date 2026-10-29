//! A fake of the Hub's file addresses, for a download in a test.
//!
//! It serves each file it holds at `/{repo}/resolve/{revision}/{path}`, as
//! the Hub's resolve endpoint does once its redirect is followed: the whole
//! file, or from a `Range: bytes=<n>-` request's `n` on, with the file's
//! SHA-256 in `X-Linked-Etag`. Armed with [`FileServer::stall_once`], it
//! sends the next whole-file answer only so far and holds the connection
//! open there, so a download can be stopped part-way.

use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

use crate::runtime::runtime;
use crate::test_hub::HubFile;

/// What the server has been asked: each request's path and `Range` header.
pub(crate) type Asked = Vec<(String, Option<String>)>;

struct Served {
    files: Vec<(String, Vec<u8>, String)>,
    asked: Mutex<Asked>,
    /// Bytes to send of the next whole-file answer before holding it.
    stall: Mutex<Option<usize>>,
    /// Give every file a digest that is not its own.
    lie: std::sync::atomic::AtomicBool,
    /// Lets every held answer go.
    release: Notify,
}

/// A running fake file server.
pub(crate) struct FileServer {
    pub(crate) url: String,
    served: Arc<Served>,
}

impl FileServer {
    /// A server of `files`, listening on a port of its own on the library's
    /// runtime.
    pub(crate) fn start(files: &[HubFile]) -> Self {
        let served = Arc::new(Served {
            files: files
                .iter()
                .map(|file| {
                    (
                        format!("{}/{}", file.repo, file.path),
                        file.bytes.clone(),
                        file.digest(),
                    )
                })
                .collect(),
            asked: Mutex::new(Vec::new()),
            stall: Mutex::new(None),
            lie: std::sync::atomic::AtomicBool::new(false),
            release: Notify::new(),
        });
        let listener = runtime()
            .block_on(TcpListener::bind("127.0.0.1:0"))
            .expect("a port on loopback");
        let url = format!("http://{}", listener.local_addr().expect("a bound port"));
        let serving = Arc::clone(&served);
        runtime().spawn(async move {
            while let Ok((request, stream)) = gglib_test_http::accept(&listener).await {
                let served = Arc::clone(&serving);
                tokio::spawn(answer(served, request, stream));
            }
        });
        Self { url, served }
    }

    /// Send only the first `bytes` of the next whole-file answer, and hold
    /// the rest until [`Self::release`].
    pub(crate) fn stall_once(&self, bytes: usize) {
        *self
            .served
            .stall
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(bytes);
    }

    /// Give every file the digest of no bytes, as a corrupted transfer
    /// would read against the true one.
    pub(crate) fn lie_about_digests(&self) {
        self.served
            .lie
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Let every held answer finish.
    pub(crate) fn release(&self) {
        self.served.release.notify_waiters();
    }

    /// Each request so far: its path and its `Range` header.
    pub(crate) fn asked(&self) -> Asked {
        self.served
            .asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

async fn answer(served: Arc<Served>, request: gglib_test_http::Request, mut stream: TcpStream) {
    let range = request.header("range").map(str::to_owned);
    served
        .asked
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((request.path.clone(), range.clone()));
    let held = served.files.iter().find(|(key, ..)| {
        request
            .path
            .strip_prefix('/')
            .and_then(|path| path.split_once("/resolve/"))
            .and_then(|(repo, rest)| Some(format!("{repo}/{}", rest.split_once('/')?.1)))
            .is_some_and(|wanted| &wanted == key)
    });
    let Some((_, bytes, digest)) = held else {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    };
    let from = range
        .as_deref()
        .and_then(|range| range.strip_prefix("bytes="))
        .and_then(|range| range.strip_suffix('-'))
        .and_then(|start| start.parse::<usize>().ok())
        .filter(|start| *start < bytes.len());
    let total = bytes.len();
    let digest = if served.lie.load(std::sync::atomic::Ordering::SeqCst) {
        crate::test_hub::HubFile {
            repo: String::new(),
            path: String::new(),
            bytes: Vec::new(),
        }
        .digest()
    } else {
        digest.clone()
    };
    let (status, body, extra) = from.map_or_else(
        || ("200 OK", &bytes[..], String::new()),
        |start| {
            (
                "206 Partial Content",
                &bytes[start..],
                format!("Content-Range: bytes {start}-{}/{total}\r\n", total - 1),
            )
        },
    );
    let response_head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}X-Linked-Etag: \"{digest}\"\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(response_head.as_bytes()).await.is_err() {
        return;
    }
    let stall = if from.is_none() {
        served
            .stall
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    } else {
        None
    };
    match stall {
        Some(sent) => {
            let sent = sent.min(body.len());
            if stream.write_all(&body[..sent]).await.is_err() {
                return;
            }
            let _ = stream.flush().await;
            served.release.notified().await;
            let _ = stream.write_all(&body[sent..]).await;
        }
        None => {
            let _ = stream.write_all(body).await;
        }
    }
    let _ = stream.shutdown().await;
}
