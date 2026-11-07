//! A turn on one of this machine's models goes through this machine's
//! endpoint: to the door's root, with its key, naming the model by its
//! catalogue id. It holds nothing, and its turns say the context its model
//! was launched with. The endpoint is turned on when it is off; one whose
//! port another process holds refuses the turn in its own words.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_core::domain::agent::{AgentEvent, MADE_KEYS, TurnUsage};
use gglib_core::ports::{
    Admission, AdmissionLease, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunningTarget,
};
use gglib_runtime::Door;
use gglib_runtime::proxy::ProxyStatus;
use serde_json::json;

use super::{endpoint, this_machines_door};
use crate::error::HttpError;
use crate::handlers::agent::hub_model::Loaded;
use crate::handlers::agent::run::with_code;
use crate::handlers::agent::run_fixture::{
    End, conversation, device, meta, prepared, saved, saving, settled, state,
};
use crate::handlers::agent::turn_fixture::model;
use crate::state::AppState;

/// Where the model is served in these tests.
const PORT: u16 = 19_555;

/// A runtime whose primary slot holds `primary`, by its port and id,
/// launched at a context of 8192. It counts the holds asked of it.
#[derive(Debug, Default)]
struct Primary {
    primary: Option<(u16, u32)>,
    holds: AtomicUsize,
}

#[async_trait]
impl ModelRuntimePort for Primary {
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
        let (port, id) = self.primary?;
        Some(RunningTarget::local(
            port,
            id,
            "qwen".to_owned(),
            8192,
            false,
        ))
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }

    fn hold(&self, _port: u16, _model_id: u32) -> Option<AdmissionLease> {
        self.holds.fetch_add(1, Ordering::SeqCst);
        None
    }
}

/// A context whose runtime is `runtime`, with a catalogued model: its id.
async fn on(runtime: &Arc<Primary>) -> (tempfile::TempDir, AppState, i64) {
    let (dir, mut state) = state().await;
    Arc::get_mut(&mut state).expect("not yet shared").runtime =
        Arc::clone(runtime) as Arc<dyn ModelRuntimePort>;
    let id = model(&state, |_| {}).await;
    (dir, state, id)
}

/// The model `model_id`, found or loaded on [`PORT`].
const fn served(model_id: i64) -> Loaded {
    Loaded {
        id: model_id,
        port: PORT,
    }
}

/// This machine's endpoint, as `ProxyOps::door` opens it.
fn door() -> Door {
    Door {
        root_url: "http://127.0.0.1:9931".to_owned(),
        key: Some("sk-this-machines-endpoint".to_owned()),
        name: None,
    }
}

fn turn() -> crate::handlers::agent::AgentChatRequest {
    serde_json::from_str(r#"{"port":19555,"messages":[]}"#).unwrap()
}

fn as_u32(id: i64) -> u32 {
    u32::try_from(id).unwrap()
}

/// The loop posts to the door's root with its key, and names the model by
/// its catalogue id, never by nothing: the endpoint resolves an id first.
#[tokio::test]
async fn a_turn_dials_the_endpoint_with_its_key_and_the_models_id() {
    let runtime = Arc::new(Primary::default());
    let (_dir, state, id) = on(&runtime).await;

    let upstream = endpoint(&state, &turn(), served(id), door()).await.unwrap();

    assert_eq!(upstream.base_url, "http://127.0.0.1:9931");
    assert_eq!(upstream.model, Some(id.to_string()));
    let dialled = upstream.door.expect("through a door");
    assert_eq!(dialled.key.as_deref(), Some("sk-this-machines-endpoint"));
    assert!(!dialled.is_far(), "this machine's own endpoint");
    assert_eq!(upstream.local_model, Some((PORT, id)));
    // The catalogue's name for the id, never what a port says it serves.
    assert_eq!(upstream.made_by.model, "catalogue-name");
    assert_eq!(upstream.counted_as, "catalogue-name");
}

/// Once its model is loaded the turn reads the context it was launched with,
/// for the size its turns report (#1272), and takes no hold: the endpoint
/// admits each request.
#[tokio::test]
async fn a_turn_reports_its_models_launched_context_and_holds_nothing() {
    let runtime = Arc::new(Primary::default());
    let (_dir, mut state, id) = on(&runtime).await;
    let primary = Arc::new(Primary {
        primary: Some((PORT, as_u32(id))),
        ..Primary::default()
    });
    Arc::get_mut(&mut state).expect("not yet shared").runtime =
        Arc::clone(&primary) as Arc<dyn ModelRuntimePort>;

    let upstream = endpoint(&state, &turn(), served(id), door()).await.unwrap();

    assert_eq!(upstream.made_by.context_size, Some(8192));
    assert_eq!(primary.holds.load(Ordering::SeqCst), 0, "a hold was taken");
}

/// The primary slot's context is another model's unless both its port and
/// its id are the turn's: a size that is not the turn's own is left out.
#[tokio::test]
async fn another_model_in_the_primary_slot_gives_no_context() {
    let runtime = Arc::new(Primary::default());
    let (_dir, mut state, id) = on(&runtime).await;
    let other = as_u32(id) + 1;
    for primary in [Some((PORT, other)), Some((PORT + 1, as_u32(id))), None] {
        Arc::get_mut(&mut state).expect("not yet shared").runtime = Arc::new(Primary {
            primary,
            ..Primary::default()
        });

        let upstream = endpoint(&state, &turn(), served(id), door()).await.unwrap();

        assert_eq!(upstream.made_by.context_size, None, "{primary:?}");
    }
}

/// A paired device's turn on a hub chat saves the context its model was
/// launched with on its reply, read as `late_on` reads it once the model is
/// loaded.
#[tokio::test]
async fn a_hub_chats_turn_saves_its_models_launched_context() {
    let runtime = Arc::new(Primary::default());
    let (_dir, mut state, id) = on(&runtime).await;
    Arc::get_mut(&mut state).expect("not yet shared").runtime = Arc::new(Primary {
        primary: Some((PORT, as_u32(id))),
        ..Primary::default()
    });
    let chat = conversation(&state).await;
    let reply = vec![
        AgentEvent::TurnUsage(TurnUsage::default()),
        AgentEvent::FinalAnswer {
            content: "Done.".to_owned(),
        },
    ];
    let (mut p, _) = prepared(reply, End::Finish);
    let upstream = endpoint(&state, &turn(), served(id), door()).await.unwrap();
    p.local_model = upstream.local_model;
    p.made_by = upstream.made_by;

    super::super::hub_turn::begin(
        &state,
        device("phone"),
        "d1",
        "qwen".to_owned(),
        saving(chat),
        super::super::launch::ready(p),
        super::super::compose::take_permit(&state).expect("a slot"),
    )
    .unwrap();
    settled(&state).await;

    let rows = saved(&state, chat).await;
    let saved_reply = rows.last().expect("the reply was saved");
    assert_eq!(meta(saved_reply, MADE_KEYS.context_size), json!(8192));
}

/// An endpoint that is off is turned on where the settings say, as starting
/// a model turns it on, and dialled there.
#[tokio::test]
async fn an_endpoint_that_is_off_is_turned_on_and_dialled() {
    let (_dir, state) = state().await;
    let free = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    store_port(&state, port).await;

    let opened = this_machines_door(&state).await;
    let running = state.proxy.status().await;
    state.proxy.stop().await.expect("stop");

    let opened = opened.expect("a door");
    assert_eq!(opened.root_url, format!("http://127.0.0.1:{port}"));
    assert!(!opened.is_far());
    assert!(matches!(running, ProxyStatus::Running { .. }), "{running}");
}

/// An endpoint whose port another process holds cannot be turned on: the
/// turn is refused with the endpoint's own reason, coded `conflict`.
#[tokio::test]
async fn an_endpoint_whose_port_another_process_holds_refuses_the_turn() {
    let (_dir, state) = state().await;
    let held = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = held.local_addr().unwrap().port();
    store_port(&state, port).await;

    let refused = this_machines_door(&state).await.map_err(with_code);

    let Err(HttpError::Coded { code, message, .. }) = refused else {
        panic!("refused, coded");
    };
    assert_eq!(code, "conflict");
    assert!(
        message.contains(&format!("{port} is already in use")),
        "{message}"
    );
    assert!(matches!(state.proxy.status().await, ProxyStatus::Stopped));
    drop(held);
}

/// Store `port` as the endpoint's.
async fn store_port(state: &AppState, port: u16) {
    let update = gglib_core::SettingsUpdate {
        proxy_port: Some(Some(port)),
        ..gglib_core::SettingsUpdate::default()
    };
    state.core.settings().update(update).await.unwrap();
}
