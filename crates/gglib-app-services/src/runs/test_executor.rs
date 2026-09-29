//! A scripted executor and a hand-driven clock, for the registry's tests.
//!
//! Each run's script is a channel the test feeds; a run with no script, or
//! whose channel is closed, waits forever, which is what a cancellation test
//! wants. Every started execution is counted, and each holds a guard whose
//! drop is recorded, so a test can see an upstream being dropped.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use gglib_core::domain::runs::{RunError, RunInfo};
use gglib_core::ports::{RunEvent, RunEvents};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::Clock;
use super::executor::{RunExecutor, RunLog};
use super::registry::RunRegistry;

/// One instruction to a scripted run.
pub(super) enum Cmd {
    Start,
    Frame(String),
    Finish(Result<(), RunError>),
}

/// Counts drops of the future it lives in.
struct DropGuard(Arc<AtomicUsize>);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(super) struct Scripted {
    scripts: Mutex<HashMap<String, mpsc::UnboundedReceiver<Cmd>>>,
    pub(super) started: AtomicUsize,
    pub(super) dropped: Arc<AtomicUsize>,
}

impl Scripted {
    /// The channel that drives the run with this id, once it is created.
    pub(super) fn script(&self, id: &str) -> mpsc::UnboundedSender<Cmd> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.scripts.lock().unwrap().insert(id.to_owned(), rx);
        tx
    }
}

#[async_trait]
impl RunExecutor for Scripted {
    async fn execute(&self, _body: Value, log: RunLog) -> Result<(), RunError> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let _guard = DropGuard(Arc::clone(&self.dropped));
        let script = self.scripts.lock().unwrap().remove(log.id());
        let Some(mut script) = script else {
            return std::future::pending().await;
        };
        while let Some(cmd) = script.recv().await {
            match cmd {
                Cmd::Start => log.started(),
                Cmd::Frame(frame) => {
                    if log.append(frame).is_err() {
                        return std::future::pending().await;
                    }
                }
                Cmd::Finish(outcome) => return outcome,
            }
        }
        std::future::pending().await
    }
}

/// A clock the test moves by hand.
#[derive(Clone, Default)]
pub(super) struct HandClock(Arc<AtomicU64>);

impl HandClock {
    pub(super) fn at(ms: u64) -> Self {
        Self(Arc::new(AtomicU64::new(ms)))
    }

    pub(super) fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }

    pub(super) fn clock(&self) -> Clock {
        let now = Arc::clone(&self.0);
        Arc::new(move || now.load(Ordering::SeqCst))
    }
}

/// A registry over a scripted executor, at a hand clock starting at 1000.
pub(super) fn registry() -> (RunRegistry, Arc<Scripted>, HandClock) {
    let executor = Arc::new(Scripted::default());
    let clock = HandClock::at(1_000);
    let registry = RunRegistry::new(Arc::clone(&executor) as Arc<dyn RunExecutor>, clock.clock());
    (registry, executor, clock)
}

/// A chat body naming `model`.
pub(super) fn body(model: &str) -> Value {
    json!({ "model": model, "messages": [{ "role": "user", "content": "hi" }] })
}

/// Read a stream to its end: the frames, and the final state if it came.
pub(super) async fn drain(mut events: RunEvents) -> (Vec<(u32, String)>, Option<RunInfo>) {
    let mut frames = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.next().await {
            match event {
                RunEvent::Frame { seq, data } => frames.push((seq, data.to_string())),
                RunEvent::End(info) => return Some(info),
            }
        }
        None
    })
    .await
    .expect("the stream ends within five seconds");
    (frames, read)
}

/// The next item of a stream, within a second.
pub(super) async fn next(events: &mut RunEvents) -> Option<RunEvent> {
    tokio::time::timeout(Duration::from_secs(1), events.next())
        .await
        .expect("an event within a second")
}

/// Wait, up to a second, for `done` to hold.
pub(super) async fn until(mut done: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the condition holds within a second");
}
