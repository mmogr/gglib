//! A loopback stand-in for `sd-server`, for tests: [`FakeSdServer`].
//!
//! It answers `GET /v1/models` the way sd-server does, and holds every render
//! request (`POST /sdcpp/v1/img_gen`, `POST /v1/images/generations`) open
//! without answering, as a real render holds the server for a minute or more.
//! Each connection is its own task, so a held render never delays a probe:
//! that is the shape of sd-server's thread pool, where `/v1/models` takes no
//! lock and a render takes `sd_ctx_mutex`.
//!
//! Given a [`JobScript`] it serves the async job API instead: `POST
//! /sdcpp/v1/img_gen`, `GET /sdcpp/v1/jobs/{id}` (one scripted answer per
//! read, the last repeating) and `POST /sdcpp/v1/jobs/{id}/cancel`, keeping
//! what it was sent and how often it was read.
//!
//! It reads each request's head before it answers: a server that answers and
//! drops the socket unread can reset it, and the client then loses the reply.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What sd-server 228c707's `/v1/models` answers, whatever it loaded.
pub(crate) const SD_MODELS_BODY: &str =
    r#"{"object":"list","data":[{"id":"sd-cpp-local","object":"model","owned_by":"local"}]}"#;

/// What the fake's async job API answers, each as a status and a body.
#[derive(Debug, Clone)]
pub(crate) struct JobScript {
    /// `POST /sdcpp/v1/img_gen`.
    pub(crate) submit: (u16, String),
    /// `GET /sdcpp/v1/jobs/{id}`, one per read; the last repeats.
    pub(crate) polls: Vec<(u16, String)>,
    /// `POST /sdcpp/v1/jobs/{id}/cancel`.
    pub(crate) cancel: (u16, String),
}

/// What the job API was sent.
#[derive(Debug, Default)]
struct JobLog {
    submitted: Vec<String>,
    polled: Vec<String>,
    cancelled: Vec<String>,
}

/// A script and what it has been sent.
type Jobs = Arc<Mutex<(JobScript, JobLog)>>;

/// The fake, listening on a loopback port.
pub(crate) struct FakeSdServer {
    /// The port it listens on.
    pub(crate) port: u16,
    held: Arc<AtomicUsize>,
    jobs: Option<Jobs>,
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

    /// An sd-server whose job API answers from `script`.
    pub(crate) async fn serve_jobs(script: JobScript) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let jobs = Arc::new(Mutex::new((script, JobLog::default())));
        Self::listen_with(listener, SD_MODELS_BODY, Some(jobs))
    }

    fn listen(listener: TcpListener, body: &'static str) -> Self {
        Self::listen_with(listener, body, None)
    }

    fn listen_with(listener: TcpListener, body: &'static str, jobs: Option<Jobs>) -> Self {
        let port = listener.local_addr().expect("an address").port();
        let held = Arc::new(AtomicUsize::new(0));
        let renders = Arc::clone(&held);
        let script = jobs.clone();
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(answer(socket, body, Arc::clone(&renders), script.clone()));
            }
        });
        Self { port, held, jobs }
    }

    /// The bodies `POST /sdcpp/v1/img_gen` was sent, in order.
    pub(crate) fn submitted(&self) -> Vec<String> {
        self.log(|log| log.submitted.clone())
    }

    /// The paths of every job read, in order.
    pub(crate) fn polled(&self) -> Vec<String> {
        self.log(|log| log.polled.clone())
    }

    /// The paths of every cancel, in order.
    pub(crate) fn cancelled(&self) -> Vec<String> {
        self.log(|log| log.cancelled.clone())
    }

    fn log<T>(&self, read: impl FnOnce(&JobLog) -> T) -> T {
        let jobs = self.jobs.as_ref().expect("a fake serving the job API");
        read(&jobs.lock().unwrap().1)
    }

    /// How many render requests it is holding open now.
    pub(crate) fn renders_held(&self) -> usize {
        self.held.load(Ordering::SeqCst)
    }
}

async fn answer(
    mut socket: TcpStream,
    models_body: &'static str,
    renders: Arc<AtomicUsize>,
    jobs: Option<Jobs>,
) {
    let Some((method, path, length)) = read_head(&mut socket).await else {
        return;
    };
    if let Some(jobs) = jobs
        && let Some((status, body)) = job_answer(&mut socket, &jobs, &method, &path, length).await
    {
        reply(&mut socket, status, &body).await;
        return;
    }
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

/// The job API's answer to a request, from the script, after reading its
/// body; `None` for a path the job API does not serve.
async fn job_answer(
    socket: &mut TcpStream,
    jobs: &Jobs,
    method: &str,
    path: &str,
    length: usize,
) -> Option<(u16, String)> {
    let mut body = vec![0u8; length];
    socket.read_exact(&mut body).await.ok()?;
    let mut guard = jobs.lock().unwrap();
    let (script, log) = &mut *guard;
    let answer = match (method, path) {
        ("POST", "/sdcpp/v1/img_gen") => {
            log.submitted
                .push(String::from_utf8_lossy(&body).into_owned());
            Some(script.submit.clone())
        }
        ("POST", p) if p.starts_with("/sdcpp/v1/jobs/") && p.ends_with("/cancel") => {
            log.cancelled.push(p.to_owned());
            Some(script.cancel.clone())
        }
        ("GET", p) if p.starts_with("/sdcpp/v1/jobs/") => {
            let at = log.polled.len().min(script.polls.len().saturating_sub(1));
            log.polled.push(p.to_owned());
            script.polls.get(at).cloned()
        }
        _ => None,
    };
    drop(guard);
    answer
}

async fn reply(socket: &mut TcpStream, status: u16, body: &str) {
    let head = format!(
        "HTTP/1.1 {status} Scripted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = socket.write_all(head.as_bytes()).await;
    let _ = socket.write_all(body.as_bytes()).await;
    let _ = socket.shutdown().await;
}

/// Read a request head to its blank line; its method, path and
/// `Content-Length` (0 when it has none).
async fn read_head(socket: &mut TcpStream) -> Option<(String, String, usize)> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if socket.read(&mut byte).await.ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head);
    let length = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    let mut words = text.split_whitespace();
    Some((words.next()?.to_owned(), words.next()?.to_owned(), length))
}
