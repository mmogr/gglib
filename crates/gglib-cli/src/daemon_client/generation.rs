//! The daemon's generation gate, seen from `gglib chat`'s own process.
//!
//! A chat here sends to llama-server's port itself, so the daemon's gate
//! cannot count its generations unless it asks. It asks through
//! `GET /api/generation/turn`, whose connection is the turn: the route
//! answers `granted` and then holds the turn until the connection closes.
//! [`DaemonGenerationGate::llm_turn`] returns a [`GenerationTurn`] that owns
//! that connection, and ending the turn closes it, so a session that dies
//! frees its turn with its socket.
//!
//! No daemon, or one that does not answer, means nothing to wait for: the
//! turn is a no-op, and the session is told once. A render turn is never
//! granted here; this CLI draws through the daemon, which takes its own.

use std::fmt;
use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures_util::StreamExt as _;
use gglib_core::contracts::http::daemon::{
    GENERATION_TURN_GRANTED, GENERATION_TURN_REFUSED, GENERATION_TURN_WAITING,
};
use gglib_core::ports::{
    AdmissionLease, GateError, GateRelease, GateWait, GateWaitObserver, GenerationGate,
    GenerationTurn, TurnKind, WaitReason,
};
use gglib_core::sse::{DataFrames, Event};
use serde::Deserialize;
use tokio::task::AbortHandle;

use super::{DaemonHandle, paths};

/// The daemon's gate over `GET /api/generation/turn`.
pub(crate) struct DaemonGenerationGate {
    client: reqwest::Client,
    /// The route's URL, fixed when the gate is made, so every turn goes to
    /// the daemon that was found then.
    url: String,
    api_key: Option<String>,
    /// Print nothing on stderr (`-Q`).
    quiet: bool,
    /// Whether the session has been told the daemon gave no turn.
    warned: AtomicBool,
}

impl fmt::Debug for DaemonGenerationGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never the key.
        f.debug_struct("DaemonGenerationGate")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl DaemonGenerationGate {
    /// The gate of the daemon `daemon` reaches, printing its waits on
    /// stderr unless `quiet`.
    pub(crate) fn new(daemon: &DaemonHandle, quiet: bool) -> Self {
        Self {
            client: daemon.client.clone(),
            url: daemon.url(paths::GENERATION_TURN_PATH),
            api_key: daemon.api_key.clone(),
            quiet,
            warned: AtomicBool::new(false),
        }
    }

    /// A turn that holds nothing, because the daemon gave none: there is
    /// nothing to wait for. Said once a session, unless quiet.
    fn unheld(&self, why: &str) -> GenerationTurn {
        if !self.warned.swap(true, Ordering::Relaxed) && !self.quiet {
            eprintln!(
                "  note: the daemon gave this chat no generation turn ({why}); its replies \
                 will not wait for an image render"
            );
        }
        GenerationTurn::new(Arc::new(Unheld), 0, TurnKind::Llm, None)
    }
}

#[async_trait]
impl GenerationGate for DaemonGenerationGate {
    async fn llm_turn(
        &self,
        observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        let mut request = self.client.get(&self.url);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = match request.send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                return Ok(self.unheld(&format!("it answered {}", response.status())));
            }
            Err(e) => return Ok(self.unheld(&e.without_url().to_string())),
        };
        let mut bytes = response.bytes_stream();
        let mut frames = DataFrames::unbounded();
        let mut line = WaitLine::new(self.quiet);
        while let Some(Ok(chunk)) = bytes.next().await {
            for event in frames.push_events(&chunk) {
                match signal(&event) {
                    Some(Signal::Waiting(wait)) => match &observer {
                        Some(observer) => observer.waiting(wait),
                        None => line.show(wait),
                    },
                    Some(Signal::Granted) => {
                        line.clear();
                        // The connection is the turn: kept open, and read so
                        // its keep-alives never back up, until the turn ends.
                        let held =
                            tokio::spawn(
                                async move { while let Some(Ok(_)) = bytes.next().await {} },
                            );
                        let owner = Arc::new(Connection(held.abort_handle()));
                        return Ok(GenerationTurn::new(owner, 1, TurnKind::Llm, None));
                    }
                    Some(Signal::Refused(refusal)) => {
                        line.clear();
                        return Err(refusal);
                    }
                    None => {}
                }
            }
        }
        line.clear();
        Ok(self.unheld("it closed the connection before granting one"))
    }

    async fn render_turn(
        &self,
        lease: AdmissionLease,
        _observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        drop(lease);
        Err(GateError::Unavailable(
            "gglib chat draws through the daemon, which takes its own render turns".to_owned(),
        ))
    }
}

/// What one event of the route says.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    /// A render is in the way.
    Waiting(GateWait),
    /// The turn is held until the connection closes.
    Granted,
    /// No turn, and the stream ends.
    Refused(GateError),
}

#[derive(Deserialize)]
struct WaitingData {
    step: u32,
    total: u32,
    position: usize,
}

#[derive(Deserialize)]
struct RefusedData {
    message: String,
    stalled_secs: Option<u64>,
}

/// Read one event of the route; `None` for one this CLI does not know or
/// cannot read.
pub(crate) fn signal(event: &Event) -> Option<Signal> {
    match event.name.as_deref()? {
        GENERATION_TURN_WAITING => {
            let data: WaitingData = serde_json::from_str(&event.data).ok()?;
            Some(Signal::Waiting(GateWait {
                reason: WaitReason::ImageRender,
                step: data.step,
                total: data.total,
                position: data.position,
            }))
        }
        GENERATION_TURN_GRANTED => Some(Signal::Granted),
        GENERATION_TURN_REFUSED => {
            let data: RefusedData = serde_json::from_str(&event.data).ok()?;
            Some(Signal::Refused(match data.stalled_secs {
                Some(secs) => GateError::Stalled(std::time::Duration::from_secs(secs)),
                None => GateError::Unavailable(data.message),
            }))
        }
        _ => None,
    }
}

/// Ends a held turn by closing its connection.
#[derive(Debug)]
struct Connection(AbortHandle);

impl GateRelease for Connection {
    fn progress(&self, _id: u64, _step: u32, _total: u32) {}

    fn end(&self, _id: u64) {
        self.0.abort();
    }
}

/// The gate side of a turn the daemon never gave.
#[derive(Debug)]
struct Unheld;

impl GateRelease for Unheld {
    fn progress(&self, _id: u64, _step: u32, _total: u32) {}

    fn end(&self, _id: u64) {}
}

/// One stderr line, rewritten in place, saying why the reply has not
/// started.
struct WaitLine {
    quiet: bool,
    shown: bool,
}

impl WaitLine {
    const fn new(quiet: bool) -> Self {
        Self {
            quiet,
            shown: false,
        }
    }

    fn show(&mut self, wait: GateWait) {
        if self.quiet {
            return;
        }
        let text = if wait.total > 0 {
            format!(
                "waiting for an image render, step {} of {}",
                wait.step, wait.total
            )
        } else {
            "waiting for an image render".to_owned()
        };
        eprint!("\r  {text}\x1b[K");
        let _ = std::io::stderr().flush();
        self.shown = true;
    }

    fn clear(&mut self) {
        if self.shown {
            eprint!("\r\x1b[K");
            let _ = std::io::stderr().flush();
            self.shown = false;
        }
    }
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;
