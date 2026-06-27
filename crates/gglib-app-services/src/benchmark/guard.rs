//! RAII cancellation guard for benchmark SSE streams.
//!
//! [`BenchmarkTaskGuard`] wraps a [`ReceiverStream<BenchmarkEvent>`] together
//! with a [`CancellationToken`].  When the guard is dropped — either because
//! the SSE stream has been fully consumed **or** because the HTTP client
//! disconnected and the Axum response was dropped — `cancel.cancel()` fires
//! cooperatively, signalling the benchmark task to stop at the next model
//! boundary.
//!
//! # Why not `JoinHandle::abort()`?
//!
//! [`tokio::task::JoinHandle::abort`] terminates the task instantly at the
//! next `await` point.  For agents (no GPU state, no open DB run record) this
//! is acceptable.  For benchmarks it is not: an aborted task leaves VRAM
//! occupied (no `stop_current()` call) and the run record permanently stuck
//! in `Running` status.
//!
//! Using [`CancellationToken`] gives the benchmark loop a cooperative exit
//! point between models, where it can call `stop_current()` and
//! `fail_run("Aborted by user")` before returning — freeing VRAM and keeping
//! the DB consistent.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::benchmark::BenchmarkEvent;

/// Wraps a [`ReceiverStream<BenchmarkEvent>`] and a [`CancellationToken`].
///
/// Dropping this struct fires `cancel.cancel()`, cooperatively aborting the
/// background benchmark task at its next inter-model `tokio::select!` check.
pub struct BenchmarkTaskGuard {
    inner: ReceiverStream<BenchmarkEvent>,
    cancel: CancellationToken,
}

impl BenchmarkTaskGuard {
    /// Wrap `inner` and `cancel` into a cancellation-on-drop guard.
    pub fn new(inner: ReceiverStream<BenchmarkEvent>, cancel: CancellationToken) -> Self {
        Self { inner, cancel }
    }
}

impl Drop for BenchmarkTaskGuard {
    fn drop(&mut self) {
        self.cancel.cancel(); // idempotent — safe to call multiple times
    }
}

impl Stream for BenchmarkTaskGuard {
    type Item = BenchmarkEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // ReceiverStream is Unpin, so Pin::new is safe here.
        Pin::new(&mut self.inner).poll_next(cx)
    }
}
