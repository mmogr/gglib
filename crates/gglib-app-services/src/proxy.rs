//! Proxy operations for GUI backend.
//!
//! Wraps the `ProxySupervisor` to provide start/stop/status operations
//! for the OpenAI-compatible proxy.
//!
//! The `ModelRuntimePort` (wrapping the shared `ProcessManager`) is injected
//! at construction time from the composition root, ensuring that the proxy and
//! any other service that drives llama-server (e.g. benchmarking) share a
//! single manager — one admission queue over one resident set — preventing
//! VRAM contention.

use std::net::SocketAddr;
use std::sync::Arc;

use gglib_core::ApiKeySource;
use gglib_core::domain::KeyState;
use gglib_core::ports::{
    AgentRunStarter, HubChatsPort, ImageGenerationPort, ModelCatalogPort, ModelRepository,
    ModelRuntimePort, RemoteGatewayPort, RunsPort,
};
use gglib_core::services::{AppCore, CatalogPortImpl};
use gglib_mcp::McpService;
use gglib_runtime::proxy::{ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};
use tokio_util::sync::CancellationToken;

use crate::error::GuiError;
use crate::remote::key::enforced;

/// Dependencies for proxy operations.
pub struct ProxyDeps {
    pub supervisor: Arc<ProxySupervisor>,
    pub model_repo: Arc<dyn ModelRepository>,
    pub mcp: Arc<McpService>,
    pub core: Arc<AppCore>,
    /// Shared runtime port — injected from the composition root so proxy and
    /// benchmark share the same `ProcessManager`, enforcing the invariant
    /// that every llama-server on this machine lives in the same resident set.
    pub runtime: Arc<dyn ModelRuntimePort>,
}

/// Proxy operations facade.
///
/// Provides GUI-friendly interface to proxy lifecycle management.
/// The `ModelRuntimePort` is shared with other services (e.g. benchmarking)
/// via the composition root, so every llama-server runs inside the same
/// admission-controlled resident set.
pub struct ProxyOps {
    pub(crate) supervisor: Arc<ProxySupervisor>,
    model_repo: Arc<dyn ModelRepository>,
    mcp: Arc<McpService>,
    pub(crate) core: Arc<AppCore>,
    runtime: Arc<dyn ModelRuntimePort>,
    /// The daemon's shutdown token, once there is a daemon.
    ///
    /// Not a dependency, because it does not exist yet when this is built:
    /// the daemon mints its token after the service graph is assembled, then
    /// hands it over. Every proxy it starts from then on carries it, which is
    /// what lets an authenticated remote client stop the whole thing.
    daemon_cancel: std::sync::OnceLock<CancellationToken>,
    /// The remote tunnel's owner, handed over the same way and for the same
    /// reason: every proxy this starts carries it, so each can tell it when a
    /// tunnelled request arrives (ADR 0012).
    remote_gateway: std::sync::OnceLock<Arc<dyn RemoteGatewayPort>>,
    /// The daemon's runs, handed over the same way, so every proxy this
    /// starts serves a paired device its own runs. Weak because the runs
    /// reach their replies through this same `ProxyOps`.
    runs: std::sync::OnceLock<std::sync::Weak<dyn RunsPort>>,
    /// The hub's chats, handed over the same way, so every proxy this starts
    /// serves them to a paired device.
    chats: std::sync::OnceLock<Arc<dyn HubChatsPort>>,
    /// What starts a device's turn on a hub chat. Handed over by the daemon
    /// once its routes are built, since the agent loop is composed there.
    turns: std::sync::OnceLock<Arc<dyn AgentRunStarter>>,
    /// The daemon's image driver, handed over at assembly, so every proxy
    /// this starts draws at `/v1/images/generations`.
    images: std::sync::OnceLock<Arc<dyn ImageGenerationPort>>,
    /// The bearer token the running proxy actually demands, and where it came
    /// from. `None` while stopped.
    ///
    /// Kept because the tunnel has to enforce *this* token, not the one in
    /// settings: a key supplied by `--api-key` is pinned and never appears in
    /// settings, so handing the tunnel the stored value would give it a
    /// credential the proxy refuses.
    effective_key: std::sync::RwLock<Option<(String, ApiKeySource)>>,
}

impl ProxyOps {
    /// Create proxy operations with the required dependencies.
    pub fn new(deps: ProxyDeps) -> Self {
        Self {
            supervisor: deps.supervisor,
            model_repo: deps.model_repo,
            mcp: deps.mcp,
            core: deps.core,
            runtime: deps.runtime,
            daemon_cancel: std::sync::OnceLock::new(),
            remote_gateway: std::sync::OnceLock::new(),
            runs: std::sync::OnceLock::new(),
            chats: std::sync::OnceLock::new(),
            turns: std::sync::OnceLock::new(),
            images: std::sync::OnceLock::new(),
            effective_key: std::sync::RwLock::new(None),
        }
    }

    /// Hand over the daemon's shutdown token.
    ///
    /// Called once, by the daemon, as soon as it has one. A second call is
    /// ignored: the token identifies a daemon's whole lifetime, so a
    /// replacement would mean a second daemon in one process.
    pub fn bind_daemon_cancel(&self, token: CancellationToken) {
        let _ = self.daemon_cancel.set(token);
    }

    /// Hand over the remote tunnel's owner. Once, like the daemon token, and
    /// before any proxy starts — the service graph does it at assembly.
    pub fn bind_remote_gateway(&self, gateway: Arc<dyn RemoteGatewayPort>) {
        let _ = self.remote_gateway.set(gateway);
    }

    /// Hand over the daemon's runs. Once, at assembly, like the gateway.
    pub fn bind_runs(&self, runs: &Arc<dyn RunsPort>) {
        let _ = self.runs.set(Arc::downgrade(runs));
    }

    /// Hand over the hub's chats. Once, at assembly, like the runs.
    pub fn bind_chats(&self, chats: Arc<dyn HubChatsPort>) {
        let _ = self.chats.set(chats);
    }

    /// Hand over what starts a device's turn on a hub chat. Once, by the
    /// daemon, before it starts a proxy.
    pub fn bind_turns(&self, turns: Arc<dyn AgentRunStarter>) {
        let _ = self.turns.set(turns);
    }

    /// Hand over the image driver. Once, at assembly, like the runs.
    pub fn bind_images(&self, images: Arc<dyn ImageGenerationPort>) {
        let _ = self.images.set(images);
    }

    /// The daemon's runs, once bound.
    #[must_use]
    pub fn runs(&self) -> Option<Arc<dyn RunsPort>> {
        self.runs.get().and_then(std::sync::Weak::upgrade)
    }

    /// The token the running proxy demands right now, with its source, or
    /// `None` while the proxy is stopped or open.
    #[must_use]
    pub fn effective_api_key(&self) -> Option<(String, ApiKeySource)> {
        self.effective_key
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Whether a request to the running proxy must carry the API key right
    /// now: the rule the tunnel and the door apply
    /// ([`crate::remote::key::enforced`]), so the status says what the proxy
    /// does. A key it was started with or one stored since, which a proxy
    /// started open picks up once one is stored.
    ///
    /// `None` when the settings cannot be read: a proxy started open may
    /// then be enforcing a stored key this cannot see, and a guess of
    /// "none needed" would be the wrong one. The proxy itself reads the
    /// stored key through its settings cache, so for the cache's TTL after
    /// a key is stored or cleared this answer can run ahead of what the
    /// proxy enforces, as the run door's does.
    pub async fn key_state(&self) -> Option<KeyState> {
        let stored = self.core.settings().get().await.ok()?;
        let demanded = enforced(self.effective_api_key(), stored.proxy_api_key.as_deref());
        Some(KeyState::of(demanded.is_some()))
    }

    /// Resolve a pinned-launch plan for a model — the same cascade
    /// `gglib serve` runs, shared via [`crate::launch_options`].
    ///
    /// Fails with `NotFound` when the model id is stale, which is the
    /// GUI's stale-list race rather than an internal error.
    pub async fn plan_pinned(
        &self,
        model_id: i64,
        request: &crate::types::StartServerRequest,
        globals: crate::launch_options::ProxyGlobals,
    ) -> Result<crate::launch_options::PinnedLaunch, GuiError> {
        let model = crate::helpers::resolve_model(self.core.models(), model_id).await?;
        let settings = self.core.settings().get().await?;
        Ok(crate::launch_options::plan_pinned_launch(
            &model, &settings, request, globals,
        ))
    }

    /// Start the proxy server, optionally pinned to a single model.
    ///
    /// Delegates to the supervisor using the shared `ModelRuntimePort`
    /// that was injected at construction time. Returns the bound address.
    ///
    /// A fresh start defines the runtime's pinned mode outright: `Some` is
    /// `gglib serve` (every other model refused), `None` the auto-swapping
    /// proxy. The pin is applied *before* the supervisor starts accepting so
    /// no request can slip a foreign model in between, and it is only
    /// applied on a fresh start — when the proxy is already running the
    /// start fails as a conflict and the standing pin is left untouched.
    ///
    /// # Arguments
    ///
    /// * `config` - Proxy server configuration (host, port, default context)
    /// * `pin` - Pin the runtime to one model for this proxy run
    pub async fn start(
        &self,
        config: ProxyConfig,
        pin: Option<gglib_core::ports::PinnedSpec>,
    ) -> Result<SocketAddr, GuiError> {
        if let ProxyStatus::Running { address } = self.supervisor.status().await {
            return Err(GuiError::Conflict(format!(
                "Proxy already running at {address}"
            )));
        }
        if let Err(e) = self.runtime.set_pin(pin) {
            return Err(GuiError::Internal(format!(
                "could not apply model pin: {e}"
            )));
        }
        self.start_raw(config).await.map_err(Self::map_start_error)
    }

    /// Shared setup behind [`Self::start`] and [`Self::ensure_running`],
    /// returning the untranslated [`SupervisorError`].
    ///
    /// Kept untranslated (rather than eagerly mapped to [`GuiError`]) because
    /// the two callers need to react differently to `AlreadyRunning` versus
    /// `BindFailed` — see [`Self::ensure_running`].
    async fn start_raw(&self, mut config: ProxyConfig) -> Result<SocketAddr, SupervisorError> {
        // Both entry points funnel through here, so the token is attached in
        // one place rather than at each caller that happens to remember.
        if config.daemon_cancel.is_none() {
            config.daemon_cancel = self.daemon_cancel.get().cloned();
        }
        if config.remote.is_none() {
            config.remote = self.remote_gateway.get().cloned();
        }
        if config.devices.runs.is_none() {
            config.devices.runs = self.runs();
        }
        if config.devices.chats.is_none() {
            config.devices.chats = self.chats.get().cloned();
        }
        if config.devices.turns.is_none() {
            config.devices.turns = self.turns.get().cloned();
        }
        if config.devices.images.is_none() {
            config.devices.images = self.images.get().cloned();
        }
        // Create catalog port from model repository (cheap wrapper; safe to
        // recreate per call — the underlying model repository is shared).
        let catalog: Arc<dyn ModelCatalogPort> =
            Arc::new(CatalogPortImpl::new(self.model_repo.clone()));

        let runtime = Arc::clone(&self.runtime);

        self.supervisor
            .start(
                config,
                runtime,
                catalog,
                self.mcp.clone(),
                self.core.settings().repo(),
            )
            .await
            // The token the supervisor settled on is kept here and not
            // returned. These callers are the GUI and the desktop tray, which
            // read it back from settings rather than being handed a
            // credential through a start response; the one reader that needs
            // the *effective* value — the tunnel, which must enforce what the
            // proxy enforces even when that is a pinned flag — asks
            // `effective_api_key`.
            .map(|bind| {
                *self
                    .effective_key
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    bind.api_key.clone().map(|key| (key, bind.api_key_source));
                bind.addr
            })
    }

    /// Translate a [`SupervisorError`] into the [`GuiError`] a caller sees.
    ///
    /// `BindFailed` maps to `Conflict`, not `Internal`: from the OS's
    /// perspective the port is simply taken, which is exactly the shape of
    /// error a caller can act on (stop the other process, or use a different
    /// port) — the same category as `AlreadyRunning`, just discovered a layer
    /// lower. It reads as "unexpected" only when nothing else was expected to
    /// be listening there.
    fn map_start_error(e: SupervisorError) -> GuiError {
        match e {
            SupervisorError::AlreadyRunning(addr) => {
                GuiError::Conflict(format!("Proxy already running at {addr}"))
            }
            SupervisorError::BindFailed { address, reason } => GuiError::Conflict(format!(
                "Port {address} is already in use ({reason}) — likely another `gglib serve` \
                 or `gglib proxy` process. Stop it, or change the proxy port in Settings."
            )),
            SupervisorError::NotRunning => {
                GuiError::Internal("Proxy not running (unexpected)".to_string())
            }
            SupervisorError::Internal(msg) => GuiError::Internal(msg),
        }
    }

    /// Start the proxy if it is not already running, and return its address.
    ///
    /// Idempotent, unlike [`Self::start`], which reports an already-running
    /// proxy as a conflict. That distinction matters for callers who need the
    /// proxy up as a precondition rather than as the thing they were asked to
    /// do — starting a model from the GUI, for one.
    ///
    /// Reads the saved `proxy_port`/`default_context_size` settings rather
    /// than `ProxyConfig::default()`'s hardcoded 8080: a user running a
    /// standing `gglib serve`/`gglib proxy` on the default port (the
    /// Copilot-facing case this exists for) would otherwise have every GUI
    /// model start fail with a bind conflict against their own process.
    ///
    /// A concurrent starter racing this same `ProxyOps` is treated as
    /// success: the status check and the start are not atomic, so two
    /// callers can race, and both should end up with a usable address rather
    /// than one seeing a spurious conflict. A bind failure against a
    /// *foreign* process is not recoverable the same way — this supervisor
    /// never started it, so its own status stays `Stopped` — and is
    /// surfaced directly instead.
    ///
    /// # Errors
    ///
    /// Returns `GuiError::Internal` if settings cannot be loaded, and whatever
    /// `map_start_error` produces if the proxy cannot start — a bind failure or
    /// an already-running supervisor both surface as `GuiError::Conflict`.
    pub async fn ensure_running(&self) -> Result<SocketAddr, GuiError> {
        if let ProxyStatus::Running { address } = self.supervisor.status().await {
            return Ok(address);
        }

        let settings = self.core.settings().get().await?;
        // The stored answers, as every start that names nothing gets them.
        let config = crate::proxy_settings::proxy_config_from_settings(&settings);

        match self.start_raw(config).await {
            Ok(address) => Ok(address),
            Err(SupervisorError::AlreadyRunning(_)) => match self.supervisor.status().await {
                ProxyStatus::Running { address } => Ok(address),
                other => Err(GuiError::Internal(format!(
                    "proxy reported as already running but status is {other}"
                ))),
            },
            Err(e) => Err(Self::map_start_error(e)),
        }
    }

    /// The shared model runtime backing this proxy.
    ///
    /// Exposed so other services can drive models through the *same*
    /// `ProcessManager` the proxy uses, preserving the invariant that only one
    /// llama-server runs at a time system-wide.
    #[must_use]
    pub fn runtime(&self) -> &Arc<dyn ModelRuntimePort> {
        &self.runtime
    }

    /// Stop the proxy server.
    ///
    /// Clears any model pin: the pin belongs to one proxy run, and leaving
    /// it set would make the next unpinned start refuse models for no
    /// visible reason.
    pub async fn stop(&self) -> Result<(), GuiError> {
        if let Err(e) = self.runtime.set_pin(None) {
            tracing::debug!("could not clear model pin on stop: {e}");
        }
        *self
            .effective_key
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.supervisor.stop().await.map_err(|e| match e {
            SupervisorError::NotRunning => GuiError::Conflict("Proxy is not running".to_string()),
            SupervisorError::AlreadyRunning(addr) => {
                GuiError::Internal(format!("Proxy unexpectedly running at {addr}"))
            }
            SupervisorError::BindFailed { address, reason } => {
                GuiError::Internal(format!("Unexpected bind error at {address}: {reason}"))
            }
            SupervisorError::Internal(msg) => GuiError::Internal(msg),
        })
    }

    /// Get the current proxy status.
    pub async fn status(&self) -> ProxyStatus {
        self.supervisor.status().await
    }

    /// Where a client on this machine reaches the running endpoint,
    /// `http://<address>/v1` by the daemon's own dial rule, or a refusal
    /// naming the fact that it is off.
    async fn reached_base_url(&self) -> Result<String, GuiError> {
        match self.supervisor.status().await {
            ProxyStatus::Running { address } => Ok(gglib_core::access::endpoint_base_url(address)),
            ProxyStatus::Stopped | ProxyStatus::Crashed => Err(GuiError::ValidationFailed(
                crate::proxy_door::ENDPOINT_OFF.to_owned(),
            )),
        }
    }

    /// The model a request names, or the pin when it names none: the pinned
    /// endpoint serves one model, and that is the one to prove and to write
    /// recipes for. With no pin, a request must name the model.
    fn target_model(&self, gglib_id: Option<i64>) -> Result<i64, GuiError> {
        gglib_id
            .or_else(|| self.pinned().map(|pin| pin.id))
            .ok_or_else(|| {
                GuiError::ValidationFailed(
                    "no model named and none pinned: name the model (`model=<gglib id>`)"
                        .to_owned(),
                )
            })
    }

    /// The id a client sends for model `gglib_id`, as `/v1/models` lists it,
    /// and whether the model draws pictures: refused for an id the catalogue
    /// does not hold.
    fn listed(models: &[gglib_core::Model], gglib_id: i64) -> Result<(String, bool), GuiError> {
        let draws = models
            .iter()
            .find(|m| m.id == gglib_id)
            .map(|m| m.image_family.is_some());
        let listed_id = crate::models::openai_ids(models).remove(&gglib_id);
        match (listed_id, draws) {
            (Some(id), Some(draws)) => Ok((id, draws)),
            _ => Err(GuiError::NotFound {
                entity: "model",
                id: gglib_id.to_string(),
            }),
        }
    }

    /// How a client connects to the running endpoint for model `gglib_id`
    /// (the pin when `None`): the daemon's address, its key rule, the id a
    /// client sends, and the recipes per client (`proxy_clients::recipes`),
    /// written once here for `gglib up`, `gglib proxy test` and the Endpoint
    /// page.
    pub async fn client_recipes(
        &self,
        gglib_id: Option<i64>,
    ) -> Result<crate::ClientRecipes, GuiError> {
        let base_url = self.reached_base_url().await?;
        let gglib_id = self.target_model(gglib_id)?;
        let models = self.core.models().list().await?;
        let (model_id, _) = Self::listed(&models, gglib_id)?;
        let key = self.key_state().await;
        Ok(crate::proxy_clients::recipes(&crate::ClientFacts {
            base_url,
            key,
            model_id,
            gglib_id,
        }))
    }

    /// Send one real completion through the running endpoint for model
    /// `gglib_id` (the pin when `None`), with the key the endpoint demands
    /// of a request now (`remote::key::demanded`: a flag's, else the stored
    /// key however lately set, else the run's), and say who answered and in
    /// how long (`proxy_prove::prove`). The request is held open while the
    /// model loads, up to [`crate::proxy_prove::LOAD_TIMEOUT`].
    ///
    /// # Errors
    ///
    /// The endpoint off, an id the catalogue does not hold, a model that
    /// draws pictures (the proof is a chat completion, which the endpoint
    /// refuses for one), or the proof's own refusal (no answer to `/health`,
    /// a non-2xx answer, a transport error), each as the sentence every
    /// surface prints.
    pub async fn prove(&self, gglib_id: Option<i64>) -> Result<crate::ProofOutcome, GuiError> {
        let base_url = self.reached_base_url().await?;
        let gglib_id = self.target_model(gglib_id)?;
        // Read once, before the proof: the answer is named from it, and a
        // read after a proof that answered could only turn it into a failure.
        let models = self.core.models().list().await?;
        let (model_id, draws) = Self::listed(&models, gglib_id)?;
        if draws {
            return Err(GuiError::ValidationFailed(format!(
                "{model_id} draws pictures, and the proof is a chat completion: name a chat model"
            )));
        }
        let stored = self.core.settings().get().await?;
        let key =
            crate::remote::key::demanded(self.effective_api_key(), stored.proxy_api_key.as_deref());
        crate::proxy_prove::prove(
            &gglib_proxy::loopback::client(),
            &base_url,
            &model_id,
            key.as_deref(),
            &crate::proxy_prove::library_files(&models),
            crate::proxy_prove::BIND_TIMEOUT,
            crate::proxy_prove::LOAD_TIMEOUT,
        )
        .await
        .map_err(|refusal| GuiError::ValidationFailed(refusal.reason))
    }

    /// The model the shared runtime is currently pinned to, if any.
    ///
    /// `Some` while a pinned proxy run (`gglib serve`) is active; surfaced in
    /// `GET /api/proxy/status` so clients can tell the two modes apart.
    #[must_use]
    pub fn pinned(&self) -> Option<gglib_core::ports::PinnedSpec> {
        self.runtime.pinned()
    }

    /// What the running proxy was started with, for callers deciding whether a
    /// re-invocation asks for something this run can still honour.
    pub async fn started_sampling(
        &self,
    ) -> Option<(Option<gglib_core::domain::InferenceConfig>, Option<String>)> {
        self.supervisor.started_sampling().await
    }

    /// Get a watch receiver for proxy exit events.
    pub fn exit_receiver(&self) -> tokio::sync::watch::Receiver<ProxyStatus> {
        self.supervisor.exit_receiver()
    }
}
#[cfg(test)]
#[path = "proxy_tests.rs"]
mod tests;
