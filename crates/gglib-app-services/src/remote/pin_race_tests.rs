//! A `forget` racing the swap that pins a redeemed key, over a real pipe.
//!
//! Split from `pin_tests.rs`, whose helpers it shares, to keep that file
//! under its size budget.

use std::sync::Arc;
use std::time::Duration;

use super::super::enrolment;
use super::super::serve_watch_tests::ops_with_key;
use super::pin_tests::{REFUSED, invited, pair, status};

/// A forget that reaches the edge after the device redeemed but before the
/// key is pinned leaves nothing admitted: the swap finds the key gone and
/// puts nothing back.
///
/// The roster lock is held across the pairing, so the swap waits on it, and
/// let go once the forget has taken the key off the edge. The forget is the
/// one both of `RemoteOps::forget` and an unwound invite run, without the
/// second look at the edge that `RemoteOps::forget` adds.
#[tokio::test(flavor = "multi_thread")]
async fn a_forget_before_the_swap_leaves_the_key_not_admitted() {
    let (_core, _proxy, _events, ops, _arming) = ops_with_key().await;
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (_, offered) = invited(&ops).await;
    let device = offered.device.as_str();
    let serving = ops
        .live
        .lock()
        .await
        .full()
        .map(|l| Arc::clone(&l.handle))
        .expect("the tunnel is up");

    let guard = ops.roster.lock().await;
    let paired = pair(&offered, &scratch.path().join("laptop")).await;
    let release = async {
        for _ in 0..100 {
            if !serving.token_names().iter().any(|name| name == device) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(guard);
    };
    let (forgotten, ()) = tokio::join!(enrolment::forget(&ops, Some(&serving), device), release);
    // The watcher settles the invite after its swap, so a spent code means
    // the swap has run.
    let mut swapped = false;
    for _ in 0..100 {
        if !ops.status().await.pairing_active {
            swapped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let held = serving.token_names().iter().any(|name| name == device);
    let own = match &paired {
        Ok(p) => status(&p.handle, &p.api_key).await,
        Err(_) => None,
    };

    if let Ok(p) = &paired {
        p.handle.shutdown_timeout(Duration::ZERO).await;
    }
    let cleaned = ops.forget(device).await;
    let stopped = ops.disable().await;

    paired.expect("the device pairs over the pipe");
    assert!(forgotten.expect("forget"), "the device was held");
    assert!(swapped, "the invite never settled within two seconds");
    assert!(!held, "the swap put a forgotten key back on the edge");
    assert_eq!(own, Some(REFUSED), "and the device is refused");
    cleaned.expect("the cleanup forget");
    stopped.expect("disable");
}
