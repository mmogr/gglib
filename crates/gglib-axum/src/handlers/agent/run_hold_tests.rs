//! A local run holds the model its loop drives until the run ends, however
//! it ends; a remote run holds nothing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::ports::{
    Admission, AdmissionLease, AdmissionRelease, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, RunningTarget, RunsPort as _,
};

use super::remote_upstream::hold;
use super::run_fixture::{
    End, LOCAL, finished_reply, logged, prepared, reply, settled, start, state,
};

/// Counts the releases of the holds it backs.
#[derive(Debug, Default)]
struct Released(AtomicUsize);

impl AdmissionRelease for Released {
    fn release(&self, _slot: usize) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl Released {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// A runtime whose every port has a resident; it records the ports held.
#[derive(Debug, Default)]
struct Holding {
    asked: Mutex<Vec<u16>>,
    released: Arc<Released>,
}

#[async_trait]
impl ModelRuntimePort for Holding {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::Internal("not asked here".to_owned()))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn hold(&self, port: u16) -> Option<AdmissionLease> {
        self.asked.lock().unwrap().push(port);
        let released: Arc<dyn AdmissionRelease> = Arc::clone(&self.released) as _;
        Some(AdmissionLease::new(released, 0))
    }
}

#[test]
fn a_local_run_holds_the_model_on_its_port_and_a_remote_run_holds_none() {
    let runtime = Holding::default();

    assert!(hold(&runtime, true, 19_555).is_none());
    assert!(runtime.asked.lock().unwrap().is_empty(), "nothing asked");

    let held = hold(&runtime, false, 19_555);
    assert!(held.is_some());
    assert_eq!(*runtime.asked.lock().unwrap(), [19_555]);
    drop(held);
    assert_eq!(runtime.released.count(), 1);
}

#[tokio::test]
async fn a_cancelled_runs_hold_is_released_when_it_ends() {
    let (_dir, state) = state().await;
    let runtime = Holding::default();
    let (mut p, _) = prepared(reply(), End::Hang);
    p.hold = hold(&runtime, false, 19_555);

    start(&state, "h1", None, p).await;
    logged(&state, "h1", reply().len()).await;
    assert_eq!(runtime.released.count(), 0, "held while the run lives");

    state.runs.cancel(&LOCAL, "h1").unwrap();
    settled(&state).await;
    assert_eq!(runtime.released.count(), 1);
}

#[tokio::test]
async fn a_completed_runs_hold_is_released_when_it_ends() {
    let (_dir, state) = state().await;
    let runtime = Holding::default();
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.hold = hold(&runtime, false, 19_555);

    start(&state, "h1", None, p).await;
    settled(&state).await;
    assert_eq!(runtime.released.count(), 1);
}
