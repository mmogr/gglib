//! `GET /api/generation/turn`: a generation turn held for as long as the
//! connection that asked for it stays open.
//!
//! `gglib chat` sends to llama-server's port from its own process, past the
//! proxy, so no lease counts its generations, and it cannot hold a turn on
//! the daemon's gate in memory. It opens this route around each send
//! instead. The stream says `waiting` (the render in the way and the place
//! in line) whenever that changes, then `granted`, and then holds the turn
//! until the stream is dropped: the client closed the connection, or went
//! away and a keep-alive found the socket gone. So a CLI that dies frees
//! its turn within a keep-alive or two. A gate that grants no turn says
//! `refused` and ends the stream.
//!
//! The stream also ends at the daemon's shutdown, as `/api/events` does, so
//! a held turn never holds the daemon open.

use std::convert::Infallible;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use gglib_core::contracts::http::daemon::{
    GENERATION_TURN_GRANTED, GENERATION_TURN_REFUSED, GENERATION_TURN_WAITING,
};
use gglib_core::ports::{GateError, GateWait, GateWaitObserver, GenerationGate, GenerationTurn};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// How often a held turn's stream writes a keep-alive, which is how a
/// client that went away without closing is noticed.
pub(crate) const KEEP_ALIVE: Duration = Duration::from_secs(5);

/// Wait for an LLM turn on the daemon's gate and hold it while the
/// connection is open (see the [module docs](self)).
pub(crate) async fn turn(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let events = turn_events(
        Arc::clone(&state.generation_gate),
        state.daemon_shutdown.clone(),
    );
    Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
}

type Wait = Pin<Box<dyn Future<Output = Result<GenerationTurn, GateError>> + Send>>;

/// Where the stream is: waiting for the turn, holding it, or done.
enum Phase {
    Waiting {
        wait: Wait,
        waits: mpsc::Receiver<GateWait>,
        shutdown: Option<CancellationToken>,
    },
    Holding {
        turn: GenerationTurn,
        shutdown: Option<CancellationToken>,
    },
    Done,
}

/// The route's events, from a wait on `gate` to the turn's end.
fn turn_events(
    gate: Arc<dyn GenerationGate>,
    shutdown: Option<CancellationToken>,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let (tx, waits) = mpsc::channel(16);
    let observer: Arc<dyn GateWaitObserver> = Arc::new(Waits(tx));
    let wait: Wait = Box::pin(async move { gate.llm_turn(Some(observer)).await });
    let start = Phase::Waiting {
        wait,
        waits,
        shutdown,
    };
    stream::unfold(start, |phase| async move {
        match phase {
            Phase::Waiting {
                mut wait,
                mut waits,
                shutdown,
            } => {
                tokio::select! {
                    biased;
                    Some(news) = waits.recv() => {
                        let event = waiting(news);
                        Some((Ok(event), Phase::Waiting { wait, waits, shutdown }))
                    }
                    granted = &mut wait => Some(match granted {
                        Ok(turn) => {
                            let event = Event::default().event(GENERATION_TURN_GRANTED).data("{}");
                            (Ok(event), Phase::Holding { turn, shutdown })
                        }
                        Err(e) => (Ok(refused(&e)), Phase::Done),
                    }),
                }
            }
            Phase::Holding { turn, shutdown } => {
                // Held here, by the stream, until axum drops it or the
                // daemon stops.
                match shutdown {
                    Some(token) => token.cancelled_owned().await,
                    None => std::future::pending::<()>().await,
                }
                drop(turn);
                None
            }
            Phase::Done => None,
        }
    })
}

/// A `waiting` event: the render in the way, and this wait's place in line.
fn waiting(wait: GateWait) -> Event {
    let data = json!({ "step": wait.step, "total": wait.total, "position": wait.position });
    Event::default()
        .event(GENERATION_TURN_WAITING)
        .data(data.to_string())
}

/// A `refused` event: why no turn was granted, and for a stall how long the
/// wait lasted.
fn refused(error: &GateError) -> Event {
    let stalled_secs = match error {
        GateError::Stalled(waited) => Some(waited.as_secs()),
        GateError::Unavailable(_) => None,
    };
    let data = json!({ "message": error.to_string(), "stalled_secs": stalled_secs });
    Event::default()
        .event(GENERATION_TURN_REFUSED)
        .data(data.to_string())
}

/// Hands each wait report to the stream; one dropped while the stream is
/// behind costs a status line and nothing else.
struct Waits(mpsc::Sender<GateWait>);

impl fmt::Debug for Waits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Waits")
    }
}

impl GateWaitObserver for Waits {
    fn waiting(&self, wait: GateWait) {
        let _ = self.0.try_send(wait);
    }
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;
