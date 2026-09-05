#![doc = include_str!("README.md")]

mod gateway;
mod key;
mod pairing;
mod rotation;
mod types;

pub use gateway::RemoteGateway;
pub use types::{EnableRequest, Enabled, RemoteStatusSnapshot};

use std::sync::Arc;
use std::time::Duration;

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_core::services::{AppCore, SETTINGS_CACHE_TTL};
use gglib_core::{SettingsUpdate, access};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::error::GuiError;
use crate::proxy::ProxyOps;
use key::KeyDecision;
use pairing::PAIRING_TTL;
use rotation::rotation_poll;

/// How long `serve` may wait for the endpoint to reach a relay before the
/// ticket is minted. The ticket is about to be shown to a person and copied
/// once, so a few seconds here buys a ticket that carries the relay path a
/// peer behind a strict NAT needs. Expiry is not an error.
const WAIT_ONLINE: Duration = Duration::from_secs(10);

/// How long `disable` lets in-flight requests finish before cutting them.
const DRAIN: Duration = Duration::from_secs(5);

/// One live serve side and the task that keeps its token current.
struct Live {
    handle: Arc<modelpipe::ServeHandle>,
    rotation: CancellationToken,
}

/// The remote tunnel's lifecycle: the serve side of ADR 0012.
///
/// Off by default and never persisted: `enable` arms the tunnel for this
/// daemon only, and nothing brings it back on a restart.
pub struct RemoteOps {
    proxy: Arc<ProxyOps>,
    core: Arc<AppCore>,
    gateway: Arc<RemoteGateway>,
    emitter: Arc<dyn AppEventEmitter>,
    live: Mutex<Option<Live>>,
}

impl RemoteOps {
    /// Build the ops over the gateway the proxy was handed.
    pub fn new(
        proxy: Arc<ProxyOps>,
        core: Arc<AppCore>,
        gateway: Arc<RemoteGateway>,
        emitter: Arc<dyn AppEventEmitter>,
    ) -> Self {
        Self {
            proxy,
            core,
            gateway,
            emitter,
            live: Mutex::new(None),
        }
    }

    /// The gateway this owns, for the service graph to hand to `ProxyOps`.
    #[must_use]
    pub fn gateway(&self) -> Arc<RemoteGateway> {
        Arc::clone(&self.gateway)
    }

    /// Bring the tunnel up in front of the running proxy and arm a pairing.
    ///
    /// Starts the proxy if it is not running; settles the key the tunnel
    /// enforces (minting and persisting one when nothing enforces anything
    /// yet — which puts a bearer requirement on the local proxy too, see
    /// ADR 0012); binds a fresh identity; grants the pairing code once at the
    /// tunnel edge; and starts watching settings for a rotation.
    ///
    /// # Errors
    ///
    /// `Conflict` when already enabled; whatever starting the proxy returns;
    /// `Internal` when settings cannot be written or the tunnel cannot bind.
    pub async fn enable(&self, request: EnableRequest) -> Result<Enabled, GuiError> {
        let mut live = self.live.lock().await;
        if live.is_some() {
            return Err(GuiError::Conflict(
                "remote access is already enabled — `gglib remote disable` first to mint a new ticket"
                    .to_owned(),
            ));
        }

        let addr = self.proxy.ensure_running().await?;
        let (key, pinned) = self.settle_key().await?;

        let mut opts = modelpipe::ServeOptions::default();
        opts.auth = modelpipe::TokenPolicy::Supplied(key.clone());
        opts.relay = request.relay;
        // A fresh identity every time: the ticket dies with the session and
        // revocation is the restart (ADR 0012, decision 4).
        opts.identity = None;
        opts.port_mapping = false;
        opts.discovery = request.discovery;
        opts.wait_online = Some(WAIT_ONLINE);
        let handle = modelpipe::serve(&format!("http://{addr}"), opts)
            .await
            .map_err(|e| GuiError::Internal(format!("could not start the remote tunnel: {e}")))?;
        let handle = Arc::new(handle);

        let code = access::generate_pairing_code();
        handle
            .grant_once(code.clone(), PAIRING_TTL)
            .map_err(|e| GuiError::Internal(format!("could not arm the pairing code: {e}")))?;
        self.gateway
            .pairing
            .begin(code.clone(), key.clone(), PAIRING_TTL);
        self.gateway.set_mcp_allowed(request.allow_mcp);

        let rotation = CancellationToken::new();
        if !pinned {
            tokio::spawn(rotation_poll(
                Arc::clone(&self.core),
                Arc::clone(&handle),
                Arc::clone(&self.gateway),
                key,
                rotation.clone(),
            ));
        }

        let ticket = handle.ticket();
        let fingerprint = ticket.fingerprint();
        info!(ticket = %fingerprint, mcp = request.allow_mcp, "remote tunnel enabled");
        self.emitter.emit(AppEvent::remote_enabled(fingerprint));
        *live = Some(Live { handle, rotation });

        let ticket = ticket.to_string();
        Ok(Enabled {
            pairing: format!("{ticket}-{code}"),
            ticket,
            code,
            expires_in_s: PAIRING_TTL.as_secs(),
        })
    }

    /// Take the tunnel down. The ticket is dead from this moment; the key
    /// stays in settings, because authentication turns on and never off.
    ///
    /// # Errors
    ///
    /// `Conflict` when nothing is enabled.
    pub async fn disable(&self) -> Result<(), GuiError> {
        let taken = self.live.lock().await.take();
        let Some(Live { handle, rotation }) = taken else {
            return Err(GuiError::Conflict(
                "remote access is not enabled".to_owned(),
            ));
        };
        rotation.cancel();
        self.gateway.reset_session();
        if !handle.shutdown_timeout(DRAIN).await {
            warn!("remote tunnel drain hit its deadline; remaining requests were cut");
        }
        info!("remote tunnel disabled");
        self.emitter.emit(AppEvent::remote_disabled());
        Ok(())
    }

    /// A snapshot for the status surface.
    pub async fn status(&self) -> RemoteStatusSnapshot {
        let live = self.live.lock().await;
        let mut snapshot = RemoteStatusSnapshot {
            enabled: live.is_some(),
            pairing_active: self.gateway.pairing.active(),
            paired: self.gateway.paired(),
            mcp_allowed: self.gateway.mcp_allowed_now(),
            tunnelled_requests: self.gateway.tunnelled_requests(),
            last_tunnelled_ms: self.gateway.last_tunnelled_ms(),
            last_peer: self.gateway.last_peer(),
            ..RemoteStatusSnapshot::default()
        };
        if let Some(Live { handle, .. }) = live.as_ref() {
            snapshot.ticket_fingerprint = Some(handle.ticket().fingerprint());
            snapshot.path = Some(handle.status().as_str().to_owned());
            snapshot.peers = handle
                .peers()
                .into_iter()
                .map(|peer| (peer.fingerprint, peer.path.as_str().to_owned()))
                .collect();
        }
        snapshot
    }

    /// The key the tunnel enforces, minting and persisting one first when
    /// nothing enforces anything yet. Returns whether it is pinned.
    async fn settle_key(&self) -> Result<(String, bool), GuiError> {
        let settings = self
            .core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("could not read settings: {e}")))?;
        match key::decide(
            self.proxy.effective_api_key(),
            settings.proxy_api_key.as_deref(),
        ) {
            KeyDecision::Use { key, pinned } => Ok((key, pinned)),
            KeyDecision::Mint(key) => {
                self.core
                    .settings()
                    .update(SettingsUpdate {
                        proxy_api_key: Some(Some(key.clone())),
                        ..SettingsUpdate::default()
                    })
                    .await
                    .map_err(|e| GuiError::Internal(format!("could not store the API key: {e}")))?;
                // The proxy's tracking policy reads settings through a cache;
                // handing out a ticket before the local door is locked would
                // open a window the whole design exists to close.
                info!("minted an API key for the proxy; waiting for it to take effect");
                tokio::time::sleep(SETTINGS_CACHE_TTL).await;
                Ok((key, false))
            }
        }
    }
}

impl RemoteGateway {
    /// [`RemoteGatewayPort::mcp_allowed`](gglib_core::ports::RemoteGatewayPort::mcp_allowed),
    /// reachable without importing the trait.
    fn mcp_allowed_now(&self) -> bool {
        gglib_core::ports::RemoteGatewayPort::mcp_allowed(self)
    }
}
