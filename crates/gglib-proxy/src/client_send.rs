//! A streamed reply's sends to its client, each bounded, so a client that
//! stops reading is let go after [`CLIENT_SEND_TIMEOUT`] as one that closes
//! its connection is let go at once.
//!
//! A streamed reply reaches its client through a channel 32 frames deep (see
//! [`forward_chat_completion`](crate::forward::forward_chat_completion)),
//! which the HTTP server empties as fast as the client's socket takes bytes.
//! A client that closes its connection drops the channel's receiver, and the
//! next send fails at once. A client that vanishes without closing, such as a
//! LAN client whose machine sleeps mid-reply, leaves it open: once the
//! socket's buffers and the channel are full, a send waits until the operating
//! system gives up on the connection, and the request keeps its model's slot
//! for all that time (#1125).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc::Sender;
use tracing::warn;

use crate::upstream_read::STREAM_IDLE_TIMEOUT;

/// How long one send to a streaming client may wait for the client to make
/// room, before the client is treated as gone.
///
/// It times each send, not the turn: a send's wait ends when the frame ahead
/// of it leaves the channel for the client's socket, so a client that reads
/// slowly but steadily never comes near it, however long the reply; only one
/// whose connection has taken nothing for this long is let go.
///
/// It equals its twin, [`STREAM_IDLE_TIMEOUT`], which times the other side of
/// the drain: the proxy waits as long for the client to take the next frame
/// as it waits for the upstream to send one.
pub(crate) const CLIENT_SEND_TIMEOUT: Duration = STREAM_IDLE_TIMEOUT;

/// The sending half of a streamed reply's channel, each send bounded.
///
/// Once a send has waited out the bound, every later send fails at once, as
/// a send to a closed channel does, so the frames still to go are not each
/// waited on in turn.
pub(crate) struct ClientSender {
    tx: Sender<Result<Bytes, std::io::Error>>,
    bound: Duration,
    /// Set when a send waited out `bound`.
    gave_up: AtomicBool,
}

impl ClientSender {
    /// Sends to `tx`, waiting at most `bound` for room each time.
    pub(crate) const fn new(tx: Sender<Result<Bytes, std::io::Error>>, bound: Duration) -> Self {
        Self {
            tx,
            bound,
            gave_up: AtomicBool::new(false),
        }
    }

    /// Send `frame` to the client: `false` when the client has closed its
    /// connection, or has made no room for the bound. Either way it is gone,
    /// and the caller ends the turn as a departure.
    pub(crate) async fn send(&self, frame: Bytes) -> bool {
        if self.gave_up.load(Ordering::Relaxed) {
            return false;
        }
        if let Ok(sent) = tokio::time::timeout(self.bound, self.tx.send(Ok(frame))).await {
            return sent.is_ok();
        }
        let bound = self.bound;
        warn!(
            ?bound,
            "the client took nothing for {bound:?}; ending its turn"
        );
        self.gave_up.store(true, Ordering::Relaxed);
        false
    }

    /// Resolves once the client has closed its connection.
    pub(crate) async fn closed(&self) {
        self.tx.closed().await;
    }
}

#[cfg(test)]
#[path = "client_send_tests.rs"]
mod tests;
