//! A local send takes its generation turn before it is sent and holds it
//! until its reply ends; a send to another machine takes none.
//!
//! The gate is the real admission queue's where waiting is the point, and a
//! counting double where only the turn's start and end matter. The model
//! server is a socket that reads each request whole before it answers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::ports::{
    AdmissionLease, AdmissionRelease, GateError, GateRelease, GateWait, GateWaitObserver,
    GenerationGate, GenerationTurn, LlmCompletionPort as _, RetryObserver, TurnKind, WaitReason,
};
use gglib_core::retry::RetryPolicy;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use super::super::{FarMachine, LlmCompletionAdapter};
use crate::process::AdmissionQueue;

/// No test here should take anywhere near this long.
const PATIENCE: Duration = Duration::from_secs(10);

/// One SSE frame of answer text.
fn frame(text: &str) -> String {
    let delta = serde_json::json!({"choices": [{"index": 0, "delta": {"content": text}}]});
    format!("data: {delta}\n\n")
}

/// A llama-server on loopback. Each connection is counted, read whole,
/// answered with headers and a first word, and finished (a second word and
/// `[DONE]`, then closed) once `finish` has a permit for it.
struct Llama {
    base: String,
    accepted: Arc<AtomicUsize>,
    finish: Arc<Semaphore>,
}

impl Llama {
    async fn start(permits: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let accepted = Arc::new(AtomicUsize::new(0));
        let finish = Arc::new(Semaphore::new(permits));
        let (count, gate) = (Arc::clone(&accepted), Arc::clone(&finish));
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                count.fetch_add(1, Ordering::SeqCst);
                let gate = Arc::clone(&gate);
                tokio::spawn(async move {
                    read_request(&mut socket).await;
                    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                                connection: close\r\n\r\n";
                    socket.write_all(head.as_bytes()).await.unwrap();
                    socket.write_all(frame("Hel").as_bytes()).await.unwrap();
                    gate.acquire().await.unwrap().forget();
                    let rest = format!("{}data: [DONE]\n\n", frame("lo"));
                    let _ = socket.write_all(rest.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self {
            base,
            accepted,
            finish,
        }
    }

    fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}

/// Read one request's head and the body its `content-length` names.
async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut read = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0, "the request ended early");
        read.extend_from_slice(&chunk[..n]);
        let Some(start) = read.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&read[..start]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .map_or(0, |value| value.trim().parse::<usize>().unwrap());
        if read.len() >= start + 4 + length {
            return;
        }
    }
}

/// A gate that grants every LLM turn at once and counts the turns taken and
/// ended.
#[derive(Debug, Default)]
struct Counting {
    taken: AtomicUsize,
    ended: Arc<Ends>,
}

#[derive(Debug, Default)]
struct Ends(AtomicUsize);

impl GateRelease for Ends {
    fn progress(&self, _id: u64, _step: u32, _total: u32) {}

    fn end(&self, _id: u64) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl Counting {
    fn ended(&self) -> usize {
        self.ended.0.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl GenerationGate for Counting {
    async fn llm_turn(
        &self,
        _observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        let id = self.taken.fetch_add(1, Ordering::SeqCst) as u64;
        let owner = Arc::clone(&self.ended) as Arc<dyn GateRelease>;
        Ok(GenerationTurn::new(owner, id, TurnKind::Llm, None))
    }

    async fn render_turn(
        &self,
        _lease: AdmissionLease,
        _observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        unreachable!("a send takes no render turn")
    }
}

/// A gate that has waited out its deadline with nothing moving.
#[derive(Debug)]
struct Stalled;

#[async_trait]
impl GenerationGate for Stalled {
    async fn llm_turn(
        &self,
        _observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        Err(GateError::Stalled(Duration::from_mins(3)))
    }

    async fn render_turn(
        &self,
        _lease: AdmissionLease,
        _observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        unreachable!("a send takes no render turn")
    }
}

/// Keeps what a send's waits reported.
#[derive(Default)]
struct Waits(Mutex<Vec<GateWait>>);

impl RetryObserver for Waits {
    fn on_retry(&self, _attempt: u32, _delay: Duration, _reason: &str) {}

    fn on_exhausted(&self, _attempts: u32, _elapsed: Duration, _reason: &str) {}

    fn on_gate_wait(&self, wait: GateWait) {
        self.0.lock().unwrap().push(wait);
    }
}

/// The lease a render would hold; nothing to release.
#[derive(Debug)]
struct NoRelease;

impl AdmissionRelease for NoRelease {
    fn release(&self, _slot: usize) {}

    fn progress(&self, _slot: usize) {}
}

fn adapter(base: &str, gate: Arc<dyn GenerationGate>) -> LlmCompletionAdapter {
    LlmCompletionAdapter::new(base, None)
        .with_generation_gate(Some(gate))
        .with_retry_policy(RetryPolicy {
            max_attempts: 1,
            total_deadline: Duration::from_secs(1),
            ..RetryPolicy::default()
        })
}

fn hi() -> [AgentMessage; 1] {
    [AgentMessage::user("hi")]
}

/// A paired device's turn on this machine's model, while a render holds
/// the GPU: nothing is sent while it waits, it is told why, the wait lasts
/// past the send timer and the retry deadline without failing, and once the
/// render ends the send goes and is served.
#[tokio::test]
async fn a_local_send_waits_out_a_held_render_before_it_is_sent() {
    let llama = Llama::start(1).await;
    let queue = Arc::new(AdmissionQueue::new());
    let gate = queue.generation_gate();
    let lease = AdmissionLease::new(Arc::new(NoRelease), 1);
    let render = gate.render_turn(lease, None).await.expect("a lone render");

    let waits = Arc::new(Waits::default());
    let mut adapter = adapter(&llama.base, Arc::clone(&gate))
        .with_retry_observer(Some(Arc::clone(&waits) as Arc<dyn RetryObserver>));
    adapter.send_timeout_secs = 1;
    let adapter = Arc::new(adapter);
    let send = tokio::spawn(async move {
        let stream = adapter.chat_stream(&hi(), &[]).await?;
        Ok::<usize, anyhow::Error>(stream.count().await)
    });

    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert_eq!(llama.accepted(), 0, "sent while a render held the GPU");
    let told = waits.0.lock().unwrap().clone();
    assert_eq!(
        told.first().map(|w| (w.reason, w.position)),
        Some((WaitReason::ImageRender, 1)),
        "{told:?}"
    );

    drop(render);
    let served = tokio::time::timeout(PATIENCE, send)
        .await
        .expect("served once the render ends")
        .unwrap();
    assert!(served.expect("the reply") > 0);
    assert_eq!(llama.accepted(), 1);
}

/// The turn is held while the reply is read, not only until its headers
/// arrive, and ends when the reply does.
#[tokio::test]
async fn the_turn_outlives_the_headers_and_ends_with_the_reply() {
    let llama = Llama::start(0).await;
    let gate = Arc::new(Counting::default());
    let adapter = adapter(&llama.base, Arc::clone(&gate) as Arc<dyn GenerationGate>);

    let mut reply = adapter.chat_stream(&hi(), &[]).await.expect("headers");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(gate.taken.load(Ordering::SeqCst), 1);
    assert_eq!(gate.ended(), 0, "the turn ended at the headers");

    llama.finish.add_permits(1);
    let drained =
        tokio::time::timeout(PATIENCE, async { while reply.next().await.is_some() {} }).await;
    drained.expect("the reply ends");
    assert_eq!(gate.ended(), 1, "the turn outlived its reply");
    drop(reply);
    assert_eq!(gate.ended(), 1);
}

/// A reply dropped before it ends gives its turn back as it goes.
#[tokio::test]
async fn a_dropped_reply_ends_its_turn() {
    let llama = Llama::start(0).await;
    let gate = Arc::new(Counting::default());
    let adapter = adapter(&llama.base, Arc::clone(&gate) as Arc<dyn GenerationGate>);

    let reply = adapter.chat_stream(&hi(), &[]).await.expect("headers");
    assert_eq!(gate.ended(), 0);
    drop(reply);
    assert_eq!(gate.ended(), 1);
}

/// A send that fails gives its turn back at once.
#[tokio::test]
async fn a_failed_send_ends_its_turn() {
    let nobody = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", nobody.local_addr().unwrap());
    drop(nobody);
    let gate = Arc::new(Counting::default());
    let adapter = adapter(&base, Arc::clone(&gate) as Arc<dyn GenerationGate>);

    assert!(adapter.chat_stream(&hi(), &[]).await.is_err());
    assert_eq!(gate.taken.load(Ordering::SeqCst), 1);
    assert_eq!(gate.ended(), 1);
}

/// A send to another machine asks this machine's gate for nothing: that
/// machine's proxy counts it.
#[tokio::test]
async fn a_send_to_another_machine_takes_no_turn() {
    let llama = Llama::start(1).await;
    let gate = Arc::new(Counting::default());
    let far = FarMachine {
        key: "far-key".to_owned(),
        name: "desk".to_owned(),
    };
    let adapter = adapter(&llama.base, Arc::clone(&gate) as Arc<dyn GenerationGate>)
        .with_far_machine(Some(far));

    let reply = adapter.chat_stream(&hi(), &[]).await.expect("served");
    assert!(tokio::time::timeout(PATIENCE, reply.count()).await.unwrap() > 0);
    assert_eq!(llama.accepted(), 1);
    assert_eq!(
        gate.taken.load(Ordering::SeqCst),
        0,
        "a far send took a turn"
    );
}

/// A gate that grants no turn fails the send, saying why, and nothing is
/// sent.
#[tokio::test]
async fn a_stalled_gate_fails_the_send_without_sending() {
    let llama = Llama::start(1).await;
    let adapter = adapter(&llama.base, Arc::new(Stalled));

    let refused = adapter.chat_stream(&hi(), &[]).await;
    let reason = refused.err().expect("no turn, no send").to_string();
    assert!(
        reason.contains("no generation turn") && reason.contains("180s"),
        "{reason}"
    );
    assert_eq!(llama.accepted(), 0);
}
