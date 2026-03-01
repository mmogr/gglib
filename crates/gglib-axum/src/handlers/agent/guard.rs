//! RAII cancellation guard bridging `mpsc::Receiver<AgentEvent>` to a [`Stream`].
//!
//! Dropping an [`AgentTaskGuard`] immediately aborts the spawned agent loop
//! task, preventing orphaned compute when the HTTP client disconnects.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

use gglib_core::domain::agent::AgentEvent;

/// Wraps a [`ReceiverStream<AgentEvent>`] together with the [`JoinHandle`] of
/// the task that feeds it.
///
/// When this struct is dropped — either because the SSE stream reaches its
/// natural end **or** because the HTTP client disconnected and Axum dropped
/// the response — [`JoinHandle::abort`] is called immediately, cancelling the
/// background [`gglib_agent::AgentLoop`] task at its next `await` point.
///
/// This prevents the loop from running to completion (burning tokens and CPU)
/// after the consumer has gone away.
pub(super) struct AgentTaskGuard {
    pub(super) inner: ReceiverStream<AgentEvent>,
    pub(super) handle: JoinHandle<()>,
}

impl Drop for AgentTaskGuard {
    fn drop(&mut self) {
        self.handle.abort(); // idempotent — safe on already-finished handles
    }
}

impl Stream for AgentTaskGuard {
    type Item = AgentEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // ReceiverStream is Unpin, so Pin::new is safe here.
        Pin::new(&mut self.inner).poll_next(cx)
    }
}
