//! A local run holds the model its loop drives until the run ends, however
//! it ends; a remote run holds nothing. Once held, the run reads the
//! context its model was launched with, when the primary slot is its own.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::domain::agent::{AgentEvent, MADE_KEYS, TurnUsage};
use gglib_core::ports::{
    Admission, AdmissionLease, AdmissionRelease, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, RunningTarget, RunsPort as _,
};
use serde_json::json;

use gglib_app_services::types::ServerInfo;

use super::remote_upstream::{hold, hold_model, local};
use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, logged, meta, prepared, reply, saved, settled, start,
    state,
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
/// asked for. Its primary slot holds `primary`, when it holds anything.
#[derive(Debug, Default)]
struct Holding {
    asked: Mutex<Vec<(u16, u32)>>,
    released: Arc<Released>,
    primary: Option<RunningTarget>,
}

/// A [`Holding`] whose primary slot has model `id` on `port`, launched at
/// a context of 8192.
fn with_primary(port: u16, id: u32) -> Holding {
    Holding {
        primary: Some(RunningTarget::local(
            port,
            id,
            "qwen".to_owned(),
            8192,
            false,
        )),
        ..Holding::default()
    }
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
        self.primary.clone()
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

/// A far upstream names none, which `remote_upstream_tests` shows.
#[tokio::test]
async fn a_local_upstream_names_the_model_it_holds() {
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
        local(&state, &req, server).await.unwrap().local_model,
        Some((19_555, 3))
    );
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

/// A run on model 1 at port 19555, as `prepare` leaves it: not yet held.
fn local_run() -> super::compose::Prepared {
    let (mut p, _) = prepared(reply(), End::Finish);
    p.local_model = Some((19_555, 1));
    p
}

#[tokio::test]
async fn a_held_primary_model_gives_its_launched_context() {
    let runtime = with_primary(19_555, 1);
    let mut p = local_run();

    hold_model(&runtime, &mut p).await.unwrap();

    assert!(p.hold.is_some(), "held");
    assert_eq!(p.made_by.context_size, Some(8192));
}

/// The primary slot's context is another model's unless both its port and
/// its id are the run's: a size that is not the run's own is left out.
#[tokio::test]
async fn another_model_on_the_primary_slot_gives_none() {
    for (port, id) in [(19_555, 2), (19_556, 1), (19_556, 2)] {
        let runtime = with_primary(port, id);
        let mut p = local_run();

        hold_model(&runtime, &mut p).await.unwrap();

        assert!(p.hold.is_some(), "its own model is held all the same");
        assert_eq!(p.made_by.context_size, None, "model {id} on {port}");
    }
    let empty = Holding::default();
    let mut p = local_run();
    hold_model(&empty, &mut p).await.unwrap();
    assert_eq!(p.made_by.context_size, None, "nothing in the primary slot");
}

/// A run on the paired machine's model holds nothing here and reads no
/// context, whatever this machine's primary slot holds.
#[tokio::test]
async fn a_far_run_reads_no_context() {
    let runtime = with_primary(19_555, 1);
    let (mut p, _) = prepared(reply(), End::Finish);
    assert_eq!(p.local_model, None, "as a far run is prepared");

    hold_model(&runtime, &mut p).await.unwrap();

    assert!(p.hold.is_none());
    assert_eq!(p.made_by.context_size, None);
    assert!(runtime.asked.lock().unwrap().is_empty(), "nothing asked");
}

/// A run refused its hold reads nothing: the context is read only of a
/// model the run holds.
#[tokio::test]
async fn a_run_refused_its_hold_reads_no_context() {
    let runtime = with_primary(19_555, 2);
    let (mut p, _) = prepared(reply(), End::Finish);
    p.local_model = Some((19_555, 2));

    let refused = hold_model(&runtime, &mut p).await;

    assert!(matches!(
        refused,
        Err(HttpError::Coded {
            code: "unavailable",
            ..
        })
    ));
    assert_eq!(p.made_by.context_size, None);
}

/// A paired device's turn on a hub chat is held and read the same way: its
/// saved reply says the context its model was launched with.
#[tokio::test]
async fn a_hub_chats_turn_saves_its_models_launched_context() {
    let (_dir, mut state) = state().await;
    let context = Arc::get_mut(&mut state).expect("not yet shared");
    context.runtime = Arc::new(with_primary(19_555, 1));
    let id = conversation(&state).await;
    let turn = vec![
        AgentEvent::TurnUsage(TurnUsage::default()),
        AgentEvent::FinalAnswer {
            content: "Done.".to_owned(),
        },
    ];
    let (mut p, _) = prepared(turn, End::Finish);
    p.local_model = Some((19_555, 1));
    let free = super::compose::take_permit(&state);

    super::hub_turn::begin(&state, "phone", "d1", id, p, free.expect("a free slot"))
        .await
        .unwrap();
    settled(&state).await;

    let rows = saved(&state, id).await;
    let saved_reply = rows.last().expect("the reply was saved");
    assert_eq!(meta(saved_reply, MADE_KEYS.context_size), json!(8192));
}
