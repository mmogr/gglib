//! The runtime, and the adapter polled the way `UniFFI`'s scaffolding polls
//! an export: by hand, on a thread with no tokio context. A `#[tokio::test]`
//! would lend each test the very context it is about, so none of these is
//! one.

use std::any::Any;
use std::panic;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::task::{Wake, Waker};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use super::*;

/// Wakes the polling thread, which is all a waker owes a caller that polls
/// one future until it finishes.
struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// How long [`poll_on_this_thread`] waits for a wake before it gives up.
/// Far longer than any call here takes; it is there so a call that nothing
/// will ever wake, which is what a runtime with no running workers produces,
/// fails its test rather than hanging the suite.
const NEVER_WOKEN: Duration = Duration::from_secs(30);

/// Poll `future` to completion on this thread, as the Swift side of an export
/// does: inline, with a waker that only asks for another poll, and with no
/// tokio context of the thread's own.
fn poll_on_this_thread<F: Future>(future: F) -> F::Output {
    assert!(
        tokio::runtime::Handle::try_current().is_err(),
        "this thread has a tokio context, which is what these tests must not supply"
    );
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    let deadline = Instant::now() + NEVER_WOKEN;
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(
            !left.is_zero(),
            "the call was not finished after {NEVER_WOKEN:?}"
        );
        thread::park_timeout(left);
    }
}

/// Poll once, as a caller does before it stops waiting.
fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

/// What a call's body does with the runtime: spawn a task, and have it say
/// which thread it ran on.
async fn spawn_a_probe() -> Option<String> {
    tokio::spawn(async { thread::current().name().map(str::to_owned) })
        .await
        .expect("the probe task finished")
}

/// The text of a caught panic, in either of the two shapes `panic!` gives it.
fn panic_text(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("<not a string>")
}

/// Hands its cleanup to the runtime when dropped: a `tokio::spawn` from a
/// destructor, as anything holding a connection or a task may do.
struct SpawnsOnDrop(mpsc::Sender<Option<String>>);

impl Drop for SpawnsOnDrop {
    fn drop(&mut self) {
        let report = self.0.clone();
        tokio::spawn(async move {
            let _ = report.send(thread::current().name().map(str::to_owned));
        });
    }
}

/// The runtime is one runtime, not one per call: what a call spawns outlives
/// the call, and a fresh runtime per call would drop it at the end of it.
#[test]
fn the_runtime_is_the_same_one_every_time() {
    let first = std::ptr::from_ref(runtime());
    let second = std::ptr::from_ref(runtime());
    assert!(
        std::ptr::eq(first, second),
        "runtime() handed out two different runtimes"
    );
}

/// It can actually run something, which the `OnceLock` alone does not say.
#[test]
fn the_runtime_runs_a_future() {
    assert_eq!(runtime().block_on(async { 2 + 2 }), 4);
}

/// What a call spawns runs on one of this library's two workers. The body
/// itself stays on the thread that polls it, and that thread has no runtime
/// entered once the call has returned.
#[test]
fn a_task_spawned_inside_a_call_runs_on_a_library_worker() {
    let caller = thread::current().id();

    let (body_ran_on, probe_ran_on) = poll_on_this_thread(in_runtime(async {
        (thread::current().id(), spawn_a_probe().await)
    }));

    assert_eq!(probe_ran_on.as_deref(), Some(WORKER_NAME));
    assert_eq!(
        body_ran_on, caller,
        "the body was moved off its caller's thread"
    );
    assert!(
        tokio::runtime::Handle::try_current().is_err(),
        "the call left its caller's thread inside the runtime"
    );
}

/// The control for the test above: the same body, polled the same way
/// without the adapter, panics for want of a runtime. So the harness does
/// reproduce a Swift thread, which has no tokio context to lend.
#[test]
fn without_the_adapter_the_same_call_panics_for_want_of_a_runtime() {
    let caught = panic::catch_unwind(|| poll_on_this_thread(spawn_a_probe()));

    let payload = caught.expect_err("a spawn found a runtime on a thread that has none");
    let text = panic_text(&*payload);
    assert!(text.contains("no reactor running"), "{text}");
}

/// A call dropped before it finishes does no more of its work, and lets go
/// of what it held. This is what polling inline buys over spawning the body
/// onto a worker, where it would run on with nobody waiting for it.
#[test]
fn dropping_a_call_before_it_finishes_stops_its_work() {
    let finished = Arc::new(AtomicBool::new(false));
    let held = Arc::new(());
    let mut call = in_runtime({
        let finished = Arc::clone(&finished);
        let held = Arc::clone(&held);
        async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            finished.store(true, Ordering::SeqCst);
            drop(held);
        }
    });
    assert!(
        poll_once(Pin::new(&mut call)).is_pending(),
        "the body finished before it could be dropped"
    );

    drop(call);

    assert_eq!(
        Arc::strong_count(&held),
        1,
        "the dropped call still holds what it held"
    );
    thread::sleep(Duration::from_millis(200));
    assert!(
        !finished.load(Ordering::SeqCst),
        "a dropped call went on to finish its work"
    );
}

/// A call dropped unfinished is dropped with the runtime entered, so a
/// destructor inside it that spawns its cleanup finds a runtime, rather than
/// panicking on the dropping thread, and the cleanup runs on a worker.
#[test]
fn a_call_dropped_unfinished_hands_its_cleanup_to_a_library_worker() {
    let (report, cleaned_up) = mpsc::channel();
    let mut call = in_runtime(async move {
        let _cleanup = SpawnsOnDrop(report);
        std::future::pending::<()>().await;
    });
    assert!(poll_once(Pin::new(&mut call)).is_pending());

    drop(call);

    let ran_on = cleaned_up
        .recv_timeout(Duration::from_secs(5))
        .expect("the cleanup ran");
    assert_eq!(ran_on.as_deref(), Some(WORKER_NAME));
}

/// Panics the way a call's body might, as a function returning `()` so the
/// body's output has a type.
fn give_up() {
    panic!("the body gave up");
}

/// A panic in a call's body reaches the caller as that panic, and the
/// caller's thread is left with no runtime entered: the adapter's guard is
/// released on the way out, not only on a return.
#[test]
fn a_panic_inside_a_call_reaches_its_caller() {
    let caught = panic::catch_unwind(|| poll_on_this_thread(in_runtime(async { give_up() })));

    let payload = caught.expect_err("the panic did not reach the caller");
    assert_eq!(panic_text(&*payload), "the body gave up");
    assert!(
        tokio::runtime::Handle::try_current().is_err(),
        "the panic left the caller's thread inside the runtime"
    );
}
