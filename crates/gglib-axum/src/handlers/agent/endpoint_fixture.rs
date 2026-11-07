//! A daemon whose endpoint is the real proxy, started on a port the OS
//! picks, over a runtime that loads the catalogue's model onto a fake
//! llama-server: what a turn through the endpoint is tested against, end to
//! end. The runtime counts the leases it hands out and the ones released,
//! so a test can see the endpoint's request let go of the model.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use gglib_app_services::{ProxyDeps, ProxyOps, ServerDeps, ServerOps};
use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::{
    Admission, AdmissionLease, AdmissionRelease, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, PinnedSpec, ProcessHandle, RunningTarget,
};
use gglib_runtime::proxy::{ProxyConfig, ProxySupervisor};
use serde_json::Value;

use crate::state::AppState;

/// Counts the leases it backs as they are released.
#[derive(Debug, Default)]
pub(super) struct Releases(AtomicUsize);

impl AdmissionRelease for Releases {
    fn release(&self, _slot: usize) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn progress(&self, _slot: usize) {}
}

/// A runtime with one model, which it loads onto the fake llama-server at
/// `port` when it is admitted, and keeps loaded until [`Self::unload`].
#[derive(Debug)]
pub(super) struct Loader {
    port: u16,
    model: i64,
    loaded: Mutex<bool>,
    admitted: AtomicUsize,
    released: Arc<Releases>,
}

impl Loader {
    /// How many times a model was admitted, by the daemon's load and by the
    /// endpoint's requests together.
    pub(super) fn admitted(&self) -> usize {
        self.admitted.load(Ordering::SeqCst)
    }

    /// How many admissions are still held: the endpoint's requests in
    /// flight.
    pub(super) fn in_flight(&self) -> usize {
        self.admitted() - self.released.0.load(Ordering::SeqCst)
    }

    /// Unload the model, as a swap or `gglib model unload` would.
    pub(super) fn unload(&self) {
        *self.loaded.lock().unwrap() = false;
    }

    fn target(&self, just_started: bool) -> RunningTarget {
        let id = u32::try_from(self.model).unwrap();
        RunningTarget::local(self.port, id, NAME.to_owned(), 4096, just_started)
    }
}

/// The catalogue's name for the model.
pub(super) const NAME: &str = "catalogue-name";

#[async_trait]
impl ModelRuntimePort for Loader {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.admitted.fetch_add(1, Ordering::SeqCst);
        let was = std::mem::replace(&mut *self.loaded.lock().unwrap(), true);
        let owner: Arc<dyn AdmissionRelease> = Arc::clone(&self.released) as _;
        Ok(Admission {
            target: self.target(!was),
            lease: AdmissionLease::new(owner, 0),
        })
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        (*self.loaded.lock().unwrap()).then(|| self.target(false))
    }

    async fn list_running(&self) -> Vec<ProcessHandle> {
        let loaded = *self.loaded.lock().unwrap();
        let handle = ProcessHandle::new(self.model, NAME.to_owned(), None, self.port, 1);
        loaded.then_some(handle).into_iter().collect()
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.unload();
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        self.unload();
        Ok(true)
    }

    /// An endpoint started with no pin clears it, which this runtime has
    /// none of to clear.
    fn set_pin(&self, pin: Option<PinnedSpec>) -> Result<(), ModelRuntimeError> {
        match pin {
            None => Ok(()),
            Some(_) => Err(ModelRuntimeError::Internal("no pin here".to_owned())),
        }
    }
}

/// A daemon whose endpoint is running, and what stands behind it.
pub(super) struct Daemon {
    pub(super) _dir: tempfile::TempDir,
    pub(super) state: AppState,
    pub(super) loader: Arc<Loader>,
    /// The catalogue's model.
    pub(super) model: i64,
    /// Where the endpoint answers, `http://127.0.0.1:<port>`.
    pub(super) root: String,
}

/// A daemon with one catalogued model on a fake llama-server at
/// `upstream_port`, its endpoint started with `key` stored as its
/// `proxy_api_key` (none: open).
pub(super) async fn daemon(upstream_port: u16, key: Option<&str>) -> Daemon {
    let (dir, state) = super::run_fixture::state().await;
    let model = super::turn_fixture::model(&state, |entry| entry.name = NAME.to_owned()).await;
    if let Some(key) = key {
        store_key(&state, key).await;
    }
    let loader = Arc::new(Loader {
        port: upstream_port,
        model,
        loaded: Mutex::new(false),
        admitted: AtomicUsize::new(0),
        released: Arc::new(Releases::default()),
    });
    let runtime: Arc<dyn ModelRuntimePort> = Arc::clone(&loader) as _;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("gglib.db")),
        )
        .await
        .expect("the test database opens again");
    let mut context = Arc::into_inner(state).expect("the only handle");
    let proxy = Arc::new(ProxyOps::new(ProxyDeps {
        supervisor: Arc::new(ProxySupervisor::new()),
        model_repo: Arc::new(gglib_db::SqliteModelRepository::new(pool)),
        mcp: Arc::clone(&context.mcp),
        core: Arc::clone(&context.core),
        runtime: Arc::clone(&runtime),
    }));
    context.servers = Arc::new(ServerOps::new(ServerDeps {
        core: Arc::clone(&context.core),
        proxy: Arc::clone(&proxy),
        emitter: Arc::clone(&context.sse) as _,
        tool_detector: Arc::new(gglib_gguf::ToolSupportDetector::new()),
    }));
    context.proxy = proxy;
    context.runtime = runtime;
    let config = ProxyConfig {
        host: "127.0.0.1".to_owned(),
        port: 0,
        ..ProxyConfig::default()
    };
    let address = context
        .proxy
        .start(config, None)
        .await
        .expect("the endpoint starts");
    Daemon {
        _dir: dir,
        state: Arc::new(context),
        loader,
        model,
        root: format!("http://{address}"),
    }
}

/// Store `key` as the endpoint's `proxy_api_key`.
pub(super) async fn store_key(state: &AppState, key: &str) {
    let update = gglib_core::SettingsUpdate {
        proxy_api_key: Some(Some(key.to_owned())),
        ..gglib_core::SettingsUpdate::default()
    };
    state.core.settings().update(update).await.unwrap();
}

impl Daemon {
    /// A chat this machine keeps on the catalogue's model, with no message.
    pub(super) async fn chat(&self) -> i64 {
        let settings = ConversationSettings {
            model: Some(ModelRef {
                machine: Machine::Local,
                id: self.model,
            }),
            ..ConversationSettings::default()
        };
        self.state
            .core
            .chat_history()
            .create_conversation(NewConversation {
                title: "t".to_owned(),
                model_id: None,
                system_prompt: None,
                settings: Some(settings),
            })
            .await
            .unwrap()
    }

    /// The endpoint's dashboard, read with `key`.
    pub(super) async fn dashboard(&self, key: Option<&str>) -> Value {
        let mut request =
            gglib_proxy::loopback::client().get(format!("{}/v1/proxy/status", self.root));
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.expect("the endpoint answers");
        assert_eq!(response.status().as_u16(), 200, "the dashboard answers");
        response.json().await.expect("a snapshot")
    }

    /// Wait until no request of the endpoint's holds the model.
    pub(super) async fn released(&self) {
        let waited = tokio::time::timeout(Duration::from_secs(10), async {
            while self.loader.in_flight() > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(
            waited.is_ok(),
            "{} admissions still held",
            self.loader.in_flight()
        );
    }

    /// Stop the endpoint.
    pub(super) async fn stop(&self) {
        self.state.proxy.stop().await.expect("the endpoint stops");
    }
}
