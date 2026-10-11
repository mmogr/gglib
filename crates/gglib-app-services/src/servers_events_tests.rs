//! Tests for what [`super::ServerOps`] puts on the event bus when a model
//! starts, stops, or fails at either: the frames as `/api/events` sends
//! them, key order included.
//!
//! The runtime is scripted, so nothing is launched. A start still brings a
//! real proxy up, on a loopback port of its own, because the start path
//! begins with `ensure_running`.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::RuntimeKind;
use gglib_core::ports::{Admission, LaunchOverrides, ModelRuntimePort, RunningTarget};
use gglib_core::{NewModel, SettingsUpdate};
use gglib_db::{CoreFactory, setup_test_database};

use super::*;
use crate::test_support::{MockToolSupportDetector, RecordingEmitter, test_core_and_proxy_on};

const MODEL: &str = "test-model";

/// A runtime that launches nothing and answers as it was told to.
#[derive(Debug)]
struct Scripted {
    /// What `admit` hands back and `list_running` reports as running.
    target: RunningTarget,
    /// Whether `target` is in the primary slot, which `current_model`
    /// reports; an image model beside a chat model is not.
    in_primary: bool,
    /// What `admit` refuses with, if it refuses.
    admit_fails: Option<ModelRuntimeError>,
    /// What `stop_current` refuses with, if it refuses.
    stop_fails: Option<ModelRuntimeError>,
    /// Whether `target` is gone by the time `stop_model` reaches it.
    gone_at_stop: bool,
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
        self.in_primary.then(|| self.target.clone())
    }

    async fn list_running(&self) -> Vec<ProcessHandle> {
        let t = &self.target;
        let handle =
            ProcessHandle::new(i64::from(t.model_id), t.model_name.clone(), None, t.port, 0);
        vec![handle.with_runtime(t.runtime)]
    }

    /// Refused, as a stop that only reached the primary would be: only
    /// [`ModelRuntimePort::stop_model`] stops the model named.
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Err(ModelRuntimeError::Internal("stop_current".to_owned()))
    }

    async fn stop_model(&self, model_id: u32) -> Result<bool, ModelRuntimeError> {
        if self.gone_at_stop || i64::from(model_id) != i64::from(self.target.model_id) {
            return Ok(false);
        }
        self.stop_fails.clone().map_or(Ok(true), Err)
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
        Self::serving(admit_fails, stop_fails, None).await
    }

    /// [`Self::new`], with [`MODEL`] on `sd-server` in the second slot and
    /// listening on `port` when `image_on` is `Some(port)`.
    async fn serving(
        admit_fails: Option<ModelRuntimeError>,
        stop_fails: Option<ModelRuntimeError>,
        image_on: Option<u16>,
    ) -> Self {
        Self::build(admit_fails, stop_fails, image_on, false).await
    }

    /// [`Self::new`], with [`MODEL`] listed as running and gone by the time
    /// the stop reaches the runtime.
    async fn gone_at_stop() -> Self {
        Self::build(None, None, None, true).await
    }

    async fn build(
        admit_fails: Option<ModelRuntimeError>,
        stop_fails: Option<ModelRuntimeError>,
        image_on: Option<u16>,
        gone_at_stop: bool,
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

        let model_port = match image_on {
            Some(port) => port,
            None => free_port().await,
        };
        let runtime = if image_on.is_some() {
            RuntimeKind::StableDiffusion
        } else {
            RuntimeKind::Llama
        };
        let target = RunningTarget::local(
            model_port,
            u32::try_from(model_id).expect("a small id"),
            MODEL.to_owned(),
            4096,
            true,
        )
        .with_runtime(runtime);
        let (core, proxy) = test_core_and_proxy_on(
            &repos,
            Arc::new(Scripted {
                target,
                in_primary: image_on.is_none(),
                admit_fails,
                stop_fails,
                gone_at_stop,
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

/// Listed when the stop began and gone when it reached the runtime (it
/// stopped on its own, or another caller stopped it first): not running,
/// and nothing announced.
#[tokio::test]
async fn a_model_gone_by_the_time_it_is_stopped_is_not_running() {
    let fx = Fixture::gone_at_stop().await;

    let stopped = fx.ops.stop(fx.model_id).await;

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

/// An image model beside a chat model is not the primary, and a Stop still
/// reaches it, by id.
#[tokio::test]
async fn stopping_an_image_model_in_the_second_slot_stops_it_by_id() {
    let fx = Fixture::serving(None, None, Some(free_port().await)).await;

    fx.ops.stop(fx.model_id).await.expect("the stop succeeds");

    assert_eq!(
        fx.frames(),
        [format!(
            r#"{{"type":"server_stopped","modelId":{},"modelName":"test-model"}}"#,
            fx.model_id
        )]
    );
}

/// A stand-in `sd-server` on a loopback port: `/v1/models` answers with
/// sd-server's model, and anything else, `/health` included, is a 404. Each
/// request head is read before the answer.
async fn fake_sd_server() -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut buf = [0_u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let body = r#"{"object":"list","data":[{"id":"sd-cpp-local","object":"model"}]}"#;
                let reply = if head.starts_with(b"GET /v1/models ") {
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else {
                    "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                        .to_owned()
                };
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    port
}

/// The health monitor a start spawns asks the server the way its runtime
/// answers: an `sd-server`, which has no `/health`, reads as healthy.
#[tokio::test]
async fn a_started_image_model_is_monitored_as_sd_server() {
    let fx = Fixture::serving(None, None, Some(fake_sd_server().await)).await;

    fx.ops
        .start(fx.model_id, StartServerRequest::default())
        .await
        .expect("the start succeeds");
    let mut first = None;
    for _ in 0..100 {
        first = fx
            .emitter
            .events()
            .into_iter()
            .find_map(|event| match event {
                AppEvent::ServerHealthChanged { status, .. } => Some(status),
                _ => None,
            });
        if first.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    fx.ops.stop(fx.model_id).await.expect("the stop succeeds");
    fx.proxy
        .stop()
        .await
        .expect("the proxy the start brought up");

    assert_eq!(first, Some(ServerHealthStatus::Healthy));
}
