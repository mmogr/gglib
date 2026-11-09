//! A turn on one of this machine's models goes through this machine's
//! endpoint: to the door's root, with its key, naming the model by its
//! catalogue id. Its turns say the context its model was launched with. The endpoint is turned on when it is off; one whose
//! port another process holds refuses the turn in its own words.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::agent::{AgentEvent, MADE_KEYS, TurnUsage};
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunningTarget,
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
/// launched at a context of 8192.
#[derive(Debug, Default)]
struct Primary {
    primary: Option<(u16, u32)>,
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
}

/// A context whose runtime is `runtime`, with a catalogued model: its id.
async fn on(runtime: &Arc<Primary>) -> (tempfile::TempDir, AppState, i64) {
    let (dir, mut state) = state().await;
    Arc::get_mut(&mut state).expect("not yet shared").runtime =
        Arc::clone(runtime) as Arc<dyn ModelRuntimePort>;
    let id = model(&state, |m| m.variant_label = Some("Q4_K_M".to_owned())).await;
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

fn turn() -> crate::handlers::agent::loop_request::LoopRequest {
    crate::handlers::agent::loop_request::LoopRequest::default()
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
    let dialled = upstream.door;
    assert_eq!(dialled.key.as_deref(), Some("sk-this-machines-endpoint"));
    assert!(!dialled.is_far(), "this machine's own endpoint");
    assert_eq!(upstream.local_model, Some((PORT, id)));
    // The catalogue's name for the id, never what a port says it serves.
    assert_eq!(upstream.made_by.model, "catalogue-name");
    assert_eq!(upstream.counted_as, "catalogue-name");
    assert_eq!(upstream.made_by.quantization.as_deref(), Some("Q4_K_M"));
}

/// A catalogue entry that names no quantisation gives none, not a guess.
#[tokio::test]
async fn a_model_without_a_quantisation_has_none() {
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;

    let upstream = endpoint(&state, &turn(), served(id), door()).await.unwrap();

    assert_eq!(upstream.made_by.quantization, None);
}

/// Once its model is loaded the turn reads the context it was launched with,
/// for the size its turns report (#1272).
#[tokio::test]
async fn a_turn_reports_its_models_launched_context() {
    let runtime = Arc::new(Primary::default());
    let (_dir, mut state, id) = on(&runtime).await;
    let primary = Arc::new(Primary {
        primary: Some((PORT, as_u32(id))),
    });
    Arc::get_mut(&mut state).expect("not yet shared").runtime =
        Arc::clone(&primary) as Arc<dyn ModelRuntimePort>;

    let upstream = endpoint(&state, &turn(), served(id), door()).await.unwrap();

    assert_eq!(upstream.made_by.context_size, Some(8192));
}

/// The primary slot's context is another model's unless both its port and
/// its id are the turn's: a size that is not the turn's own is left out.
#[tokio::test]
async fn another_model_in_the_primary_slot_gives_no_context() {
    let runtime = Arc::new(Primary::default());
    let (_dir, mut state, id) = on(&runtime).await;
    let other = as_u32(id) + 1;
    for primary in [Some((PORT, other)), Some((PORT + 1, as_u32(id))), None] {
        Arc::get_mut(&mut state).expect("not yet shared").runtime = Arc::new(Primary { primary });

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
/// a model turns it on, and dialled there; a window showing it Off is told
/// it is on, once, and not again by a turn that finds it on.
#[tokio::test]
async fn an_endpoint_that_is_off_is_turned_on_and_dialled() {
    use futures_util::StreamExt;
    let (_dir, state) = state().await;
    let mut events = Box::pin(state.sse.events());

    // A port freed here can be taken by a test running beside this one before
    // the endpoint binds it; a refused start is tried again on another.
    let mut tries = 0;
    let (port, opened) = loop {
        let free = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        store_port(&state, port).await;
        let opened = this_machines_door(&state).await;
        tries += 1;
        if opened.is_ok() || tries == 5 {
            break (port, opened);
        }
    };
    let again = this_machines_door(&state).await;
    let running = state.proxy.status().await;
    state.proxy.stop().await.expect("stop");

    let told = tokio::time::timeout(std::time::Duration::from_secs(5), events.next())
        .await
        .expect("an event")
        .expect("the stream");
    assert_eq!(
        serde_json::to_value(&told).unwrap(),
        serde_json::to_value(gglib_core::events::AppEvent::proxy_started(port)).unwrap()
    );
    let more = tokio::time::timeout(std::time::Duration::from_millis(200), events.next()).await;
    assert!(more.is_err(), "told twice: {more:?}");
    assert!(again.is_ok());

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
