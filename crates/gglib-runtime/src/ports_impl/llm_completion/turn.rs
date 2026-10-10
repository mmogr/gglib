//! A local send's generation turn: waited for before the request leaves, and
//! held until its reply has been read.
//!
//! An image render on `sd-server` needs the GPU to itself for minutes, and a
//! llama-server generation beside it would slow both. The daemon's
//! [`GenerationGate`] orders them. A send to this machine's llama-server
//! goes past the proxy, so no proxy lease counts it; it takes an LLM turn
//! itself, here.
//!
//! The turn is waited for before the send, so neither the send timer nor the
//! retry policy's deadline is spent on the wait: a render may hold the GPU
//! for longer than either. It ends when the reply's stream ends or is
//! dropped, not when the headers arrive, since the generation runs for as
//! long as the stream does.

use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use anyhow::{Result, anyhow};
use futures_core::Stream;
use gglib_core::ports::{
    GateWait, GateWaitObserver, GenerationGate, GenerationTurn, RetryObserver,
};

/// Wait on `gate` for an LLM turn, telling `observer` how the wait goes.
///
/// `None` when there is no gate to wait on: the caller has none (a
/// benchmark), or the send is going to another machine, whose own proxy
/// counts it.
///
/// # Errors
///
/// When the gate grants no turn ([`gglib_core::ports::GateError`]); nothing
/// has been sent.
pub(super) async fn take(
    gate: Option<&Arc<dyn GenerationGate>>,
    observer: Option<&Arc<dyn RetryObserver>>,
) -> Result<Option<GenerationTurn>> {
    let Some(gate) = gate else {
        return Ok(None);
    };
    let observer = observer
        .map(|observer| Arc::new(Reported(Arc::clone(observer))) as Arc<dyn GateWaitObserver>);
    let turn = gate
        .llm_turn(observer)
        .await
        .map_err(|e| anyhow!("no generation turn for this request: {e}"))?;
    Ok(Some(turn))
}

/// `inner`, holding `turn` until it ends or is dropped.
pub(super) fn held<S>(inner: S, turn: Option<GenerationTurn>) -> TurnHeld<S> {
    TurnHeld { inner, turn }
}

/// A reply's stream that holds the turn its send took, ending the turn when
/// the stream ends or is dropped.
pub(super) struct TurnHeld<S> {
    inner: S,
    turn: Option<GenerationTurn>,
}

impl<S: Stream + Unpin> Stream for TurnHeld<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<S::Item>> {
        let item = ready!(Pin::new(&mut self.inner).poll_next(cx));
        if item.is_none() {
            // The reply is over, and so is the generation: whoever waits
            // behind this one goes now, not when the caller lets go.
            self.turn = None;
        }
        Poll::Ready(item)
    }
}

/// A wait for a turn, reported through the send's retry observer: the one
/// seam that already reaches a person watching the reply.
struct Reported(Arc<dyn RetryObserver>);

impl fmt::Debug for Reported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Reported")
    }
}

impl GateWaitObserver for Reported {
    fn waiting(&self, wait: GateWait) {
        self.0.on_gate_wait(wait);
    }
}

#[cfg(test)]
#[path = "turn_tests.rs"]
mod tests;
