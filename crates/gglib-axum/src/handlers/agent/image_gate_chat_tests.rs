//! A run for a model that draws images is refused with
//! `image_model_cannot_chat` at each daemon door, before anything is
//! admitted, held or written: a device's turn on a hub chat whose model
//! draws, and a page's run on a port an `sd-server` serves.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_app_services::types::ServerInfo;
use gglib_core::domain::chat::NewConversation;
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::{ImageFamily, NewModel, RuntimeKind};
use gglib_core::ports::{
    Admission, AdmissionLease, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, RunningTarget,
    RunsPort as _,
};

use super::super::hub_turn::start;
use super::super::remote_upstream::local;
use super::super::run_fixture::saved;
use crate::error::HttpError;
use crate::state::AppState;

/// A device's turn on chat `conversation_id`, saying `content`.
fn turn(conversation_id: i64, content: &str) -> HubTurn {
    HubTurn {
        conversation_id,
        content: content.to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
    }
}

/// A runtime that counts every admission and hold it is asked for, and
/// grants none.
#[derive(Debug, Default)]
struct Counting {
    admitted: AtomicUsize,
    held: AtomicUsize,
}

#[async_trait]
impl ModelRuntimePort for Counting {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.admitted.fetch_add(1, Ordering::SeqCst);
        Err(ModelRuntimeError::Internal(
            "counted, not admitted".to_owned(),
        ))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn hold(&self, _port: u16, _model_id: u32) -> Option<AdmissionLease> {
        self.held.fetch_add(1, Ordering::SeqCst);
        None
    }
}

/// An isolated daemon whose runtime is `runtime`.
async fn state_over(runtime: Arc<Counting>) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let mut context = crate::bootstrap::bootstrap(crate::ServerConfig {
        base_port: Some(19_200),
        llama_server_path: "/nonexistent/llama-server".into(),
        sd_server_path: "/nonexistent/sd-server".into(),
        max_concurrent_agent_loops: 1,
        db_path: Some(dir.path().join("gglib.db")),
        device_keys_path: Some(dir.path().join("remote_devices")),
    })
    .await
    .expect("bootstrap an isolated context");
    context.runtime = runtime;
    (dir, Arc::new(context))
}

/// A catalogue model that draws as Flux.1.
async fn flux(state: &AppState) -> i64 {
    let mut new = NewModel::new(
        "flux".to_owned(),
        PathBuf::from("models").join("flux.gguf"),
        12.0,
        chrono::Utc::now(),
    );
    new.image_family = Some(ImageFamily::Flux1);
    state.core.models().add(new).await.expect("added").id
}

/// The code and the words of a refusal.
fn coded<T>(result: Result<T, HttpError>) -> (u16, &'static str, String) {
    match result {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => (status.as_u16(), code, message),
        Err(other) => panic!("uncoded: {other}"),
        Ok(_) => panic!("not refused"),
    }
}

#[tokio::test]
async fn a_hub_turn_on_a_chat_whose_model_draws_is_refused_before_admit() {
    let runtime = Arc::new(Counting::default());
    let (_dir, state) = state_over(Arc::clone(&runtime)).await;
    let model_id = flux(&state).await;
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: Some(model_id),
            ..NewConversation::default()
        })
        .await
        .unwrap();

    let (status, code, message) = coded(start(&state, "phone", "d1", turn(id, "hi")).await);

    assert_eq!((status, code), (400, "image_model_cannot_chat"));
    assert!(message.contains("'flux'"), "{message}");
    assert_eq!(
        runtime.admitted.load(Ordering::SeqCst),
        0,
        "admit was asked"
    );
    assert!(
        saved(&state, id).await.is_empty(),
        "the refusal wrote a row"
    );
    assert!(
        state
            .runs
            .list(&gglib_core::ports::RunScope::Device("phone".to_owned()))
            .runs
            .is_empty()
    );
}

#[tokio::test]
async fn a_page_run_on_a_port_an_sd_server_serves_is_refused_before_its_hold() {
    let runtime = Arc::new(Counting::default());
    let (_dir, state) = state_over(Arc::clone(&runtime)).await;
    let model_id = flux(&state).await;
    let req = serde_json::from_str(r#"{"port":19555,"messages":[]}"#).unwrap();
    let server = ServerInfo {
        model_id,
        model_name: "flux".to_owned(),
        pid: None,
        port: 19_555,
        started_at: 0,
        runtime: RuntimeKind::StableDiffusion,
    };

    let (status, code, message) = coded(local(&state, &req, server).await);

    assert_eq!((status, code), (400, "image_model_cannot_chat"));
    assert!(message.contains("'flux'"), "{message}");
    assert_eq!(runtime.admitted.load(Ordering::SeqCst), 0);
    assert_eq!(runtime.held.load(Ordering::SeqCst), 0);
}

/// The same door lets a llama-server's port through: the refusal is about
/// the runtime, not the port.
#[tokio::test]
async fn a_page_run_on_a_llama_server_port_goes_on() {
    let runtime = Arc::new(Counting::default());
    let (_dir, state) = state_over(runtime).await;
    let req = serde_json::from_str(r#"{"port":19555,"messages":[]}"#).unwrap();
    let server = ServerInfo {
        model_id: 3,
        model_name: "qwen".to_owned(),
        pid: None,
        port: 19_555,
        started_at: 0,
        runtime: RuntimeKind::Llama,
    };

    assert!(local(&state, &req, server).await.is_ok());
}
