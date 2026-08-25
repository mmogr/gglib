//! Tests for the failed-launch guard in [`super`].
//!
//! Split out of `launch.rs` to keep it under the repo's file-size ratchet;
//! see `scripts/check_rust_complexity.sh`. Unix-only for the harmless binary
//! they spawn, not for the behaviour.

use super::{LIVENESS_TICK, SpawnedChild};
use crate::process::core::GuiProcessCore;
use gglib_core::ports::ServerConfig;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Ids far outside anything a real catalog hands out.
///
/// `spawn` writes a pidfile keyed by model id, and in a debug build
/// `pids_dir()` resolves into the checkout itself rather than a temp
/// directory — so a low id here would collide with a real model's pidfile on
/// the developer's own machine, and `delete_pidfile` would remove it, leaving
/// a live server the startup sweep can no longer reap.
///
/// Redirecting `GGLIB_DATA_DIR` would isolate this properly, but setting an
/// environment variable is `unsafe` in this edition and the workspace denies
/// `unsafe_code`. Implausible ids plus an unconditional `kill` in every test
/// that spawns achieve the same isolation without an exemption.
const ARMED_ID: u32 = 999_001;
const DISARMED_ID: u32 = 999_002;
const NO_RUNTIME_ID: u32 = 999_003;
const CANCELLED_ID: u32 = 999_004;
const ABA_ID: u32 = 999_005;

/// A core wired to a harmless binary, plus a model file that exists so
/// `spawn`'s own existence check passes.
fn spawnable(model_id: i64) -> (Arc<RwLock<GuiProcessCore>>, ServerConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let model = dir.path().join("model.gguf");
    std::fs::write(&model, b"not really a gguf").expect("write model file");
    let core = Arc::new(RwLock::new(GuiProcessCore::new(19080, "/usr/bin/true")));
    // 19080 is the *base* port to allocate from, not the port itself:
    // `ServerConfig::new` leaves `port: None`.
    let config = ServerConfig::new(model_id, "test-model".to_owned(), model, 19080);
    (core, config, dir)
}

/// Cancellation is the case an error arm cannot reach: `run_launch` is
/// wrapped in `tokio::time::timeout`, which drops the future mid-await.
/// Dropping an armed guard must still stop the child — and, through
/// `kill`, remove its pidfile.
#[tokio::test]
async fn dropping_an_armed_guard_stops_the_child() {
    let (core, config, _dir) = spawnable(i64::from(ARMED_ID));
    let (_, pid) = core.write().await.spawn(config).await.expect("spawn");
    assert_eq!(core.read().await.count(), 1);

    drop(SpawnedChild::arm(&core, ARMED_ID, pid));

    // The kill is detached, so wait for it to land rather than assuming.
    for _ in 0..200 {
        if core.read().await.count() == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Leave nothing behind before failing.
    core.write().await.kill(ARMED_ID).await.ok();
    panic!("an armed guard must stop the child it owns");
}

/// Once the queue owns the process, the guard must keep its hands off —
/// otherwise a successful launch kills its own server on the way out.
#[tokio::test]
async fn a_disarmed_guard_leaves_the_child_alone() {
    let (core, config, _dir) = spawnable(i64::from(DISARMED_ID));
    let (_, pid) = core.write().await.spawn(config).await.expect("spawn");

    let mut guard = SpawnedChild::arm(&core, DISARMED_ID, pid);
    guard.disarm();
    drop(guard);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let survived = core.read().await.count();

    // Always clean up: `spawn` wrote a pidfile into the checkout, and
    // `kill` is what removes it.
    core.write().await.kill(DISARMED_ID).await.ok();

    assert_eq!(survived, 1, "a disarmed guard must not stop the child");
}

/// A guard dropped with no reactor must warn rather than panic: a panic in
/// `Drop` would be worse than the leak it is preventing.
#[test]
fn dropping_outside_a_runtime_does_not_panic() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let core = Arc::new(RwLock::new(GuiProcessCore::new(19090, "/usr/bin/true")));
    // Armed *inside* the runtime, dropped after it is gone — the real
    // shutdown scenario, not merely a guard built on a bare thread.
    let guard = rt.block_on(async { SpawnedChild::arm(&core, NO_RUNTIME_ID, u32::MAX) });
    drop(rt);
    drop(guard);
}

/// The liveness tick has to be short relative to the shortest launch
/// budget, or a startup crash is still reported at the deadline.
#[test]
fn the_liveness_tick_is_short_next_to_the_smallest_budget() {
    let floor = crate::process::health::launch_deadline_secs(0);
    assert!(
        LIVENESS_TICK.as_secs() * 10 < floor,
        "a {}s tick is not short next to a {floor}s floor",
        LIVENESS_TICK.as_secs()
    );
}

/// The regression itself: an error arm cannot see cancellation.
///
/// `run_launch` is wrapped in `tokio::time::timeout`, which drops the future
/// mid-await rather than returning — so `?`, `if let Err(..)` and `match` are
/// all skipped. This is the shape of a launch still waiting on `/health` when
/// its budget expires, and the child must still be stopped.
#[tokio::test]
async fn a_cancelled_launch_still_stops_the_child() {
    let (core, config, _dir) = spawnable(i64::from(CANCELLED_ID));
    let (_, pid) = core.write().await.spawn(config).await.expect("spawn");
    assert_eq!(core.read().await.count(), 1);

    let cancelled = tokio::time::timeout(std::time::Duration::from_millis(50), async {
        let _child = SpawnedChild::arm(&core, CANCELLED_ID, pid);
        std::future::pending::<()>().await;
    })
    .await;
    assert!(
        cancelled.is_err(),
        "the launch must be cancelled, not finish"
    );

    for _ in 0..200 {
        if core.read().await.count() == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    core.write().await.kill(CANCELLED_ID).await.ok();
    panic!("a cancelled launch must still stop its child");
}

/// The id is a name and names get reused: the failure path frees the slot
/// while this kill is still queued, so a retry can register a different
/// process under the same id first. The guard must not stop that one.
#[tokio::test]
async fn a_guard_does_not_stop_a_replacement_process() {
    let (core, config, _dir) = spawnable(i64::from(ABA_ID));
    let (_, original_pid) = core
        .write()
        .await
        .spawn(config.clone())
        .await
        .expect("spawn");

    let guard = SpawnedChild::arm(&core, ABA_ID, original_pid);

    // The original goes away and a *different* process takes the same id,
    // exactly as a retry would.
    core.write()
        .await
        .kill(ABA_ID)
        .await
        .expect("kill original");
    let (_, replacement_pid) = core.write().await.spawn(config).await.expect("respawn");
    assert_ne!(
        replacement_pid, original_pid,
        "precondition: the replacement must be a different process"
    );

    drop(guard);
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let survived = core.read().await.count();
    core.write().await.kill(ABA_ID).await.ok();
    assert_eq!(
        survived, 1,
        "the guard must not stop a process it never owned"
    );
}

/// The outer budget must leave room for the wait it contains. The two are
/// single-sourced now, so they cannot drift — but a zero overhead would still
/// race the timeout against the health deadline it wraps, making the reported
/// error nondeterministic and truncating the liveness check.
#[test]
fn the_launch_budget_adds_headroom_to_the_health_deadline() {
    let deadline = std::time::Duration::from_secs(600);
    assert!(crate::process::admission::launch_timeout(deadline) > deadline);
}
