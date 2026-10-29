//! The one tokio runtime this library owns, and the adapter every async
//! export runs its body in.
//!
//! `uniffi` polls an exported future inline, on whichever Swift thread awaits
//! it, with no tokio context, and an app has no `#[tokio::main]` above this
//! library. So the library owns one runtime, lazily, for the life of the
//! process: the work a call starts outlives the call, and a per-call runtime
//! would be dropped the moment the call returned, taking that work with it.
//! The crate documentation says why this and not `uniffi`'s own option.
//!
//! [`in_runtime`] is what makes it current. It enters this runtime for every
//! poll of a call's body and for the body's drop, so the body runs on the
//! calling thread while what it spawns, and the timers and sockets it
//! registers, run on this runtime's two workers.

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};

use tokio::runtime::{Builder, Runtime};

/// The process-wide runtime, built on first use.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The name each worker thread carries, so a crash report or a debugger's
/// thread list says which library a thread belongs to. An app carrying
/// several Rust libraries otherwise shows a stack of identical
/// `tokio-runtime-worker` entries.
const WORKER_NAME: &str = "gglib-ffi";

/// Borrow the runtime, building it if this is the first call.
///
/// # Panics
///
/// If the runtime cannot be built, which means the OS refused to start
/// threads. Without a runtime nothing in this crate can work, and an error
/// per call would push an unrecoverable condition into every signature.
pub(crate) fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name(WORKER_NAME)
            // Two, and the cap is the point: the work here waits on disks and
            // sockets, and the default of one thread per core would, on a
            // phone, start workers that only compete with the app's own.
            .worker_threads(2)
            .build()
            .expect("a tokio runtime; without one nothing in this library can run")
    })
}

/// Run `body` with this library's runtime entered, whichever thread polls it.
///
/// Every exported `async fn` is `in_runtime(async move { … }).await`. The body
/// is polled inline, on the caller's thread, and never handed to a worker, so
/// a call behaves as it would without the adapter: dropped unfinished, it
/// stops; a panic in it unwinds into its caller.
///
/// `Send` because `uniffi` requires the future an export becomes to be
/// `Send`; asking here names the body that is not, rather than a line of
/// generated scaffolding. `'static` so the body owns everything it uses, and
/// all of it is dropped with the runtime entered.
pub(crate) fn in_runtime<F: Future + Send + 'static>(body: F) -> InRuntime<F> {
    InRuntime {
        body: Some(Box::pin(body)),
    }
}

/// The future [`in_runtime`] returns.
pub(crate) struct InRuntime<F> {
    /// Boxed so the adapter is `Unpin` and needs no pin projection, which
    /// would take `unsafe` or a dependency. `None` only once `drop` has
    /// taken it.
    body: Option<Pin<Box<F>>>,
}

impl<F: Future> Future for InRuntime<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let _entered = runtime().enter();
        self.body
            .as_mut()
            .expect("the body is taken only by drop")
            .as_mut()
            .poll(cx)
    }
}

impl<F> Drop for InRuntime<F> {
    /// A call can be dropped before it finishes, and on any thread: a Rust
    /// caller that stops awaiting, or a binding that frees an unfinished
    /// future, which the FFI allows. What the body still holds is dropped
    /// here with the runtime entered, so a destructor that hands its cleanup
    /// to `tokio::spawn` finds a runtime to hand it to.
    fn drop(&mut self) {
        let _entered = runtime().enter();
        drop(self.body.take());
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;
