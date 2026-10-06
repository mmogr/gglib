//! Tests for what [`super::ServerOps`] puts on the event bus when a model
//! starts, stops, or fails at either: the frames as `/api/events` sends
//! them, key order included.
//!
//! The runtime is scripted, so nothing is launched. A start still brings a
//! real proxy up, on a loopback port of its own, because the start path
//! begins with `ensure_running`.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{Admission, LaunchOverrides, ModelRuntimePort, RunningTarget};
use gglib_core::{NewModel, SettingsUpdate};
use gglib_db::{CoreFactory, setup_test_database};

use super::*;
use crate::test_support::{MockToolSupportDetector, RecordingEmitter, test_core_and_proxy_on};

const MODEL: &str = "test-model";

/// A runtime that launches nothing and answers as it was told to.
#[derive(Debug)]
struct Scripted {
    /// What `admit` hands back and `current_model` reports as running.
    target: RunningTarget,
    /// What `admit` refuses with, if it refuses.
    admit_fails: Option<ModelRuntimeError>,
    /// What `stop_current` refuses with, if it refuses.
    stop_fails: Option<ModelRuntimeError>,
}

#[async_trait]
impl ModelRuntimePort for Scripted {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.admit_fails
            .clone()
            .map_or_else(|| Ok(Admission::detached(self.target.clone())), Err)
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        Some(self.target.clone())
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.stop_fails.clone().map_or(Ok(()), Err)
    }
}

/// A loopback port nothing is listening on.
async fn free_port() -> u16 {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    probe.local_addr().expect("the bound address").port()
}

/// What a test drives and reads: the ops, the model the scripted runtime
/// reports as running, and what was emitted.
struct Fixture {
    ops: ServerOps,
    proxy: Arc<crate::ProxyOps>,
    emitter: Arc<RecordingEmitter>,
    /// The library id of [`MODEL`]. Never 0 or 1: another model is added
    /// first, so the id in a frame cannot be a default or a counter's start.
    model_id: i64,
    /// The port the scripted runtime says [`MODEL`] is on.
    model_port: u16,
    _model_file: tempfile::TempDir,
}

impl Fixture {
    /// A `ServerOps` whose runtime refuses a start with `admit_fails` and a
    /// stop with `stop_fails`, and otherwise reports [`MODEL`] as running.
    async fn new(
        admit_fails: Option<ModelRuntimeError>,
        stop_fails: Option<ModelRuntimeError>,
    ) -> Self {
        let pool = setup_test_database().await.expect("in-memory DB");
        let repos = CoreFactory::build_repos(pool);

        let dir = tempfile::tempdir().expect("a directory for the model file");
        let mut model_id = 0;
        for name in ["another-model", MODEL] {
            let path = dir.path().join(format!("{name}.gguf"));
            std::fs::write(&path, b"placeholder").expect("the model file");
            let model = NewModel::new(name.to_owned(), path, 7.0, chrono::Utc::now());
            model_id = repos.models.insert(&model).await.expect("a model").id;
        }

        let model_port = free_port().await;
        let target = RunningTarget::local(
            model_port,
            u32::try_from(model_id).expect("a small id"),
            MODEL.to_owned(),
            4096,
            true,
        );
        let (core, proxy) = test_core_and_proxy_on(
            &repos,
            Arc::new(Scripted {
                target,
                admit_fails,
                stop_fails,
            }),
        );
        // Off 8080, where a developer's own daemon usually is.
        core.settings()
            .update(SettingsUpdate {
                proxy_port: Some(Some(free_port().await)),
                ..SettingsUpdate::default()
            })
            .await
            .expect("settings update");

        let emitter = Arc::new(RecordingEmitter::default());
        let ops = ServerOps::new(ServerDeps {
            core,
            proxy: Arc::clone(&proxy),
            emitter: Arc::clone(&emitter) as Arc<dyn AppEventEmitter>,
            tool_detector: Arc::new(MockToolSupportDetector),
        });
        Self {
            ops,
            proxy,
            emitter,
            model_id,
            model_port,
            _model_file: dir,
        }
    }

    /// The lifecycle frames sent so far. Health changes are left out: the
    /// monitor a start spawns reports on its own clock.
    fn frames(&self) -> Vec<String> {
        self.emitter
            .events()
            .iter()
            .filter(|event| !matches!(event, AppEvent::ServerHealthChanged { .. }))
            .map(|event| serde_json::to_string(event).expect("an event serializes"))
            .collect()
    }
}

#[tokio::test]
async fn starting_a_model_sends_server_started_with_its_id_name_and_port() {
    let fx = Fixture::new(None, None).await;

    let started = fx
        .ops
        .start(fx.model_id, StartServerRequest::default())
        .await;
    let frames = fx.frames();
    fx.proxy
        .stop()
        .await
        .expect("the proxy the start brought up");

    assert_eq!(started.expect("the start succeeds").port, fx.model_port);
    assert_eq!(
        frames,
        [format!(
            r#"{{"type":"server_started","modelId":{},"modelName":"test-model","port":{}}}"#,
            fx.model_id, fx.model_port
        )]
    );
}

#[tokio::test]
async fn a_start_the_runtime_refuses_sends_server_error_and_no_server_started() {
    let refusal = ModelRuntimeError::SpawnFailed("no binary".to_owned());
    let fx = Fixture::new(Some(refusal), None).await;

    let started = fx
        .ops
        .start(fx.model_id, StartServerRequest::default())
        .await;
    let frames = fx.frames();
    fx.proxy
        .stop()
        .await
        .expect("the proxy the start brought up");

    assert!(started.is_err(), "{started:?}");
    assert_eq!(
        frames,
        [format!(
            r#"{{"type":"server_error","modelId":{},"modelName":"test-model","error":{{"message":"Failed to start model: no binary","type":"server_error","retryable":false}}}}"#,
            fx.model_id
        )]
    );
}

#[tokio::test]
async fn stopping_a_model_sends_server_stopped_with_its_id_and_name() {
    let fx = Fixture::new(None, None).await;

    fx.ops.stop(fx.model_id).await.expect("the stop succeeds");

    assert_eq!(
        fx.frames(),
        [format!(
            r#"{{"type":"server_stopped","modelId":{},"modelName":"test-model"}}"#,
            fx.model_id
        )]
    );
}

#[tokio::test]
async fn a_stop_the_runtime_refuses_sends_server_error_and_no_server_stopped() {
    let fx = Fixture::new(None, Some(ModelRuntimeError::ModelLoading)).await;

    let stopped = fx.ops.stop(fx.model_id).await;

    assert!(matches!(stopped, Err(GuiError::Internal(_))), "{stopped:?}");
    assert_eq!(
        fx.frames(),
        [format!(
            r#"{{"type":"server_error","modelId":{},"modelName":"test-model","error":{{"message":"Model is loading, try again","type":"service_unavailable","retryable":true}}}}"#,
            fx.model_id
        )]
    );
}

/// The runtime has one model up and the stop names another: nothing is
/// stopped, so nothing is announced.
#[tokio::test]
async fn stopping_a_model_that_is_not_the_one_running_sends_nothing() {
    let fx = Fixture::new(None, None).await;

    let stopped = fx.ops.stop(fx.model_id - 1).await;

    assert!(
        matches!(
            stopped,
            Err(GuiError::NotFound {
                entity: "server",
                ..
            })
        ),
        "{stopped:?}"
    );
    assert_eq!(fx.frames(), Vec::<String>::new());
}
