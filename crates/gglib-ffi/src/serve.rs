//! The engine an app serves: gglib's own proxy on a port of loopback, in
//! front of the in-process runtime that fits one model at a time to the
//! device and decodes it in this process.
//!
//! Composed as the agentic eval's proxy arm composes an in-process proxy
//! (`gglib-app-services`' `benchmark/proxy_arm.rs`), for a phone: the
//! runtime is `InProcessRuntime` over the decoder this build has, the
//! catalog is the library's own models, the MCP service has the library's
//! table of servers (a phone adds none), and the settings are the library's.
//! There is no prompt cache, no slot folder, no host fact (a phone has no
//! name to publish, and the proxy sizes nothing on disk), and no dashboard
//! publisher. Every request must carry the serve's own bearer key: 32 bytes
//! from the OS's random source, new for each serve, since every app on a
//! phone shares its loopback.
//!
//! The runtime's budget is made from the one reading the app hands over and
//! held for the whole serve, so every model it admits is fitted to the same
//! number (`domain::device_budget`).

use std::fmt;
use std::fmt::Write as _;
use std::net::Ipv4Addr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::ports::{
    McpServerRepository, ModelCatalogPort, ModelRepository, ModelRuntimePort, SettingsRepository,
};
use gglib_core::services::CatalogPortImpl;
use gglib_core::{ApiKeySource, ProxyAccessConfig};
use gglib_engine_llama::{Decoder, InProcessRuntime};
use gglib_mcp::McpService;
use gglib_proxy::slot_eviction::DiskBudget;
use gglib_proxy::{HostFacts, ProxyObservers, ServeConfig};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::engine_watch::EngineWatch;
use crate::errors::GglibError;
use crate::reading::DeviceReading;
use crate::runtime::{in_runtime, runtime};

/// How long a shutdown waits for the proxy's last connections once the
/// engine behind them has stopped, before it stops the proxy's task itself.
const PROXY_WAIT: Duration = Duration::from_secs(5);

/// The random bytes in a serve's bearer key, written as hex.
const KEY_BYTES: usize = 32;

/// What a served engine is composed over, from the library serving it.
pub(crate) struct Composition {
    pub(crate) models: Arc<dyn ModelRepository>,
    pub(crate) mcp_servers: Arc<dyn McpServerRepository>,
    pub(crate) settings: Arc<dyn SettingsRepository>,
}

/// A served engine: gglib's proxy at [`Self::base_url`], admitting requests
/// that carry [`Self::api_key`] as their bearer.
///
/// No model is loaded until a request names one; [`Self::status`] says how
/// that goes. [`Self::shutdown`] stops it, and a server the app lets go of
/// without one is stopped all the same.
#[derive(uniffi::Object)]
pub struct Server {
    base_url: String,
    api_key: String,
    engine: Arc<InProcessRuntime>,
    proxy: CancellationToken,
    /// The proxy's task, until a shutdown takes it. Held for the whole of
    /// a shutdown, so a second one returns once the first has finished.
    running: Mutex<Option<JoinHandle<()>>>,
}

/// What a library served last, held weakly: the server while the app holds
/// it, and its engine while anything does, as the task stopping a server let
/// go of does until the engine's model is freed.
#[derive(Default)]
pub(crate) struct LastServed {
    server: Weak<Server>,
    engine: Weak<InProcessRuntime>,
}

impl LastServed {
    /// `server`, served last.
    pub(crate) fn of(server: &Arc<Server>) -> Self {
        Self {
            server: Arc::downgrade(server),
            engine: Arc::downgrade(&server.engine),
        }
    }

    /// Stop what is left of it, and return once its model is freed: the
    /// server if the app still holds it, or else its engine if anything
    /// does, whose shutdown waits for one already under way.
    pub(crate) async fn stop(&self) {
        if let Some(server) = self.server.upgrade() {
            server.stop().await;
        } else if let Some(engine) = self.engine.upgrade()
            && let Err(e) = engine.shutdown().await
        {
            tracing::warn!("the engine served before did not stop cleanly: {e}");
        }
    }
}

/// Refuse `reading` when the budget cannot be made from it, with a zero in
/// it: no model could be fitted to it, so every request would be refused.
pub(crate) fn check_reading(reading: DeviceReading) -> Result<(), GglibError> {
    if reading.budget().is_some() {
        return Ok(());
    }
    Err(GglibError::Internal {
        message: "this device's memory reading has a zero in it, so no model can be fitted to \
                  it; read the memory again and serve with that"
            .to_owned(),
    })
}

/// Serve `composition` with an engine over `decoder`, its budget made from
/// `reading`, which [`check_reading`] has passed.
pub(crate) async fn start(
    decoder: impl Decoder + 'static,
    composition: &Composition,
    reading: DeviceReading,
) -> Result<Server, GglibError> {
    let api_key = mint_key()?;
    let catalog: Arc<dyn ModelCatalogPort> =
        Arc::new(CatalogPortImpl::new(Arc::clone(&composition.models)));
    let engine = InProcessRuntime::start(decoder, Arc::clone(&catalog), reading.for_core())
        .await
        .map_err(GglibError::internal)?;
    let engine = Arc::new(engine);
    let (listener, port) = match bind_loopback().await {
        Ok(bound) => bound,
        Err(e) => {
            let _ = engine.shutdown().await;
            return Err(GglibError::internal(e));
        }
    };
    let proxy = CancellationToken::new();
    let config = ServeConfig {
        listener,
        default_ctx: None,
        // The budget is the engine's, made from the app's reading.
        device_memory_readable: true,
        runtime_port: Arc::clone(&engine) as Arc<dyn ModelRuntimePort>,
        catalog_port: catalog,
        mcp: Arc::new(McpService::new(Arc::clone(&composition.mcp_servers))),
        cancel: proxy.clone(),
        daemon_cancel: None,
        settings_repo: Arc::clone(&composition.settings),
        inference_override: None,
        default_profile: None,
        cache_enabled: false,
        slot_dir: None,
        // Never read: there is no slot folder to sweep.
        disk_budget: DiskBudget::Auto,
        host: HostFacts::none(),
        dashboard: false,
        agent_metrics: Arc::new(CacheMetricsStore::new()),
        observers: ProxyObservers::default(),
        access: ProxyAccessConfig {
            api_key: Some(api_key.clone()),
            api_key_source: ApiKeySource::Flag,
            ..ProxyAccessConfig::default()
        },
    };
    // The proxy can fail before it accepts only at the listener's address,
    // which `bind_loopback` has read already, or at building its HTTP client.
    // An engine behind a proxy that stopped holds no model, since nothing
    // can admit one, and the app's request is refused at the port.
    let task = tokio::spawn(async move {
        if let Err(e) = gglib_proxy::serve(config).await {
            tracing::warn!("the served engine's proxy stopped with an error: {e:#}");
        }
    });
    Ok(Server {
        base_url: format!("http://{}:{port}/v1", Ipv4Addr::LOCALHOST),
        api_key,
        engine,
        proxy,
        running: Mutex::new(Some(task)),
    })
}

/// A listener on a free port of 127.0.0.1, and the port.
async fn bind_loopback() -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// A new bearer key: [`KEY_BYTES`] from the OS's random source, as hex.
fn mint_key() -> Result<String, GglibError> {
    let mut bytes = [0u8; KEY_BYTES];
    getrandom::fill(&mut bytes).map_err(GglibError::internal)?;
    let mut key = String::with_capacity(KEY_BYTES * 2);
    for byte in bytes {
        let _ = write!(key, "{byte:02x}");
    }
    Ok(key)
}

impl Server {
    /// Stop serving, once: the proxy takes no new connection, the engine
    /// ends the reply in flight, frees the model and stops listening, its
    /// watch ends, and the proxy's last connections close. Returns at once
    /// when that is done already, and after it when it is under way.
    pub(crate) async fn stop(&self) {
        let mut running = self.running.lock().await;
        let Some(task) = running.take() else {
            return;
        };
        self.proxy.cancel();
        if let Err(e) = self.engine.shutdown().await {
            tracing::warn!("the served engine did not stop cleanly: {e}");
        }
        let abort = task.abort_handle();
        if tokio::time::timeout(PROXY_WAIT, task).await.is_err() {
            tracing::warn!("the served engine's proxy did not stop within {PROXY_WAIT:?}");
            abort.abort();
        }
        // Held until here, so a second call returns once this one is done.
        drop(running);
    }
}

#[uniffi::export]
impl Server {
    /// `http://127.0.0.1:<port>/v1`, where an OpenAI-compatible client
    /// sends its requests.
    pub fn base_url(&self) -> String {
        self.base_url.clone()
    }

    /// The bearer every request must carry, new for each serve.
    pub fn api_key(&self) -> String {
        self.api_key.clone()
    }

    /// A watch of what the engine is doing, from where it stands now.
    pub fn status(&self) -> Arc<EngineWatch> {
        Arc::new(EngineWatch::new(self.engine.subscribe()))
    }

    /// Stop serving: end the reply in flight, free the model, close both
    /// ports (the proxy's and the engine's behind it), and end every watch
    /// on [`crate::EngineState::Stopped`]. Idempotent; returns once all of
    /// that is done.
    pub async fn shutdown(self: Arc<Self>) {
        in_runtime(async move { self.stop().await }).await;
    }
}

impl Drop for Server {
    /// A server let go of while it still serves is stopped on the library's
    /// threads, so a model is never left loaded behind a server nobody
    /// holds.
    fn drop(&mut self) {
        if self.running.get_mut().take().is_none() {
            return;
        }
        self.proxy.cancel();
        let engine = Arc::clone(&self.engine);
        runtime().spawn(async move {
            if let Err(e) = engine.shutdown().await {
                tracing::warn!("a served engine let go of did not stop cleanly: {e}");
            }
        });
    }
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("base_url", &self.base_url)
            .field("engine", &self.engine)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
