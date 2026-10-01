//! A local run holds the model its loop drives until the run ends, however
//! it ends; a remote run holds nothing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::ports::{
    Admission, AdmissionLease, AdmissionRelease, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, RunningTarget, RunsPort as _,
};

use gglib_app_services::types::ServerInfo;

use super::remote_upstream::{hold, local, remote};
use super::run_fixture::{
    End, LOCAL, finished_reply, logged, prepared, reply, settled, start, state,
};
use crate::error::HttpError;

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

/// A runtime whose every port has model 1 resident; it records each hold
/// asked for.
#[derive(Debug, Default)]
struct Holding {
    asked: Mutex<Vec<(u16, u32)>>,
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

    fn hold(&self, port: u16, model_id: u32) -> Option<AdmissionLease> {
        self.asked.lock().unwrap().push((port, model_id));
        let released: Arc<dyn AdmissionRelease> = Arc::clone(&self.released) as _;
        (model_id == 1).then(|| AdmissionLease::new(released, 0))
    }
}

#[test]
fn a_local_run_holds_the_model_it_resolved_and_a_remote_run_holds_none() {
    let runtime = Holding::default();

    assert!(hold(&runtime, None).unwrap().is_none());
    assert!(runtime.asked.lock().unwrap().is_empty(), "nothing asked");

    let held = hold(&runtime, Some((19_555, 1))).unwrap();
    assert!(held.is_some());
    assert_eq!(*runtime.asked.lock().unwrap(), [(19_555, 1)]);
    drop(held);
    assert_eq!(runtime.released.count(), 1);
}

/// A swap between resolving the model and holding it refuses the run,
/// retryably, rather than running it unheld or on another model.
#[test]
fn a_run_whose_model_left_its_port_is_refused() {
    let runtime = Holding::default();

    let Err(HttpError::Coded { status, code, .. }) = hold(&runtime, Some((19_555, 2))) else {
        panic!("refused");
    };
    assert_eq!((status.as_u16(), code), (503, "unavailable"));
}

#[tokio::test]
async fn a_local_upstream_names_its_model_and_a_remote_one_none() {
    let (_dir, state) = state().await;
    let req = serde_json::from_str(r#"{"port":19555,"messages":[]}"#).unwrap();
    let server = ServerInfo {
        model_id: 3,
        model_name: "qwen".to_owned(),
        pid: None,
        port: 19_555,
        started_at: 0,
    };
    assert_eq!(
        local(&state, &req, server).await.local_model,
        Some((19_555, 3))
    );

    let far = remote("qwen".to_owned(), 9000, "fp".to_owned(), "key".to_owned());
    assert_eq!(far.local_model, None);
}

#[tokio::test]
async fn a_cancelled_runs_hold_is_released_when_it_ends() {
    let (_dir, state) = state().await;
    let runtime = Holding::default();
    let (mut p, _) = prepared(reply(), End::Hang);
    p.hold = hold(&runtime, Some((19_555, 1))).unwrap();

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
    p.hold = hold(&runtime, Some((19_555, 1))).unwrap();

    start(&state, "h1", None, p).await;
    settled(&state).await;
    assert_eq!(runtime.released.count(), 1);
}

/// A run whose llama-server went silent ends in error, and its hold goes with
/// it, so a silent server does not keep the model held (#1212).
#[tokio::test]
async fn a_stalled_runs_hold_is_released_when_it_ends() {
    let (_dir, state) = state().await;
    let runtime = Holding::default();
    let (mut p, _) = prepared(reply(), End::Stalled);
    p.hold = hold(&runtime, Some((19_555, 1))).unwrap();

    start(&state, "h1", None, p).await;
    settled(&state).await;
    let error = state.runs.get(&LOCAL, "h1").unwrap().error.unwrap();
    assert_eq!(error.code, "agent_error");
    assert_eq!(runtime.released.count(), 1);
}
