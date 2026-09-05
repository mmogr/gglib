//! The connect side of ADR 0012: this machine reaching another.
//!
//! `RemoteOps` owns both halves of the tunnel, and this file is the half
//! where this daemon is the laptop. The local listener it binds does **not**
//! inject `Authorization` (ADR 0012, decision 7): gglib's own commands attach
//! the stored key, and a third-party client pointed at the port supplies it
//! as its API key.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gglib_core::SettingsUpdate;
use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use modelpipe::{ConnectError, ConnectHandle, PipeStatus};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::pairing_string::{self, Parsed};
use super::types::{ConnectRequest, ConnectSnapshot, Connected};
use super::{RemoteOps, redeem};
use crate::error::GuiError;

/// How long `disconnect` lets in-flight requests finish before cutting them.
const DRAIN: Duration = Duration::from_secs(5);

/// One live connect side and the task watching it.
pub(super) struct LiveConnect {
    handle: Arc<ConnectHandle>,
    ticket_fingerprint: String,
    /// Which `connect` this is, so a watcher that outlives its connection
    /// cannot take down the next one.
    generation: u64,
    watcher: CancellationToken,
}

impl RemoteOps {
    /// Reach another machine: bind a loopback port here that is its proxy.
    ///
    /// With a `<ticket>-<code>` pairing, redeems the code through the tunnel
    /// for the far machine's API key and stores it as `remote_api_key`
    /// alongside the ticket, so later sessions need only the ticket — or
    /// nothing, since the ticket is remembered too.
    ///
    /// # Errors
    ///
    /// `Conflict` when already connected; `ValidationFailed` for a pairing
    /// string that does not parse, a bare ticket when no key is stored, or a
    /// code the far side refuses; `Unavailable` when the peer cannot be
    /// reached; `Internal` when settings cannot be written.
    pub async fn connect(&self, request: ConnectRequest) -> Result<Connected, GuiError> {
        let mut live = self.live_connect.lock().await;
        if live.is_some() {
            return Err(GuiError::Conflict(
                "already connected to a remote — `gglib remote disconnect` first".to_owned(),
            ));
        }
        let settings = self.settings().await?;
        let Parsed { ticket, code } = match request.pairing.as_deref() {
            Some(pairing) => pairing_string::parse(pairing).map_err(GuiError::ValidationFailed)?,
            None => {
                let stored = settings.remote_last_ticket.as_deref().ok_or_else(|| {
                    GuiError::ValidationFailed(
                        "this machine has not connected to a remote before — give it the pairing \
                         string `gglib remote enable` showed there"
                            .to_owned(),
                    )
                })?;
                pairing_string::parse(stored).map_err(GuiError::ValidationFailed)?
            }
        };
        let stored_key = settings
            .remote_api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty());
        if code.is_none() && stored_key.is_none() {
            return Err(GuiError::ValidationFailed(
                "this machine holds no key for that remote — pair once with the full \
                 `<ticket>-<code>` string from `gglib remote enable`"
                    .to_owned(),
            ));
        }

        let mut opts = modelpipe::ConnectOptions::default();
        opts.bind = request
            .port
            .map(|port| SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
        opts.relay = request.relay;
        opts.port_mapping = false;
        opts.discovery = request.discovery;
        let handle = Arc::new(
            modelpipe::connect(&ticket, opts)
                .await
                .map_err(|e| connect_error(e, request.port))?,
        );
        let base_url = handle.base_url();

        let paired = match code {
            Some(code) => {
                let key = match redeem::redeem(&base_url, &code).await {
                    Ok(key) => key,
                    Err(e) => {
                        handle.shutdown_timeout(Duration::from_secs(1)).await;
                        return Err(e);
                    }
                };
                self.remember(Some(key), ticket.to_string()).await?;
                true
            }
            None => {
                if settings.remote_last_ticket.as_deref() != Some(&ticket.to_string()) {
                    self.remember(None, ticket.to_string()).await?;
                }
                false
            }
        };

        let port = handle.local_addr().port();
        let ticket_fingerprint = ticket.fingerprint();
        let generation = self.connect_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let watcher = CancellationToken::new();
        tokio::spawn(watch(
            Arc::clone(&self.live_connect),
            Arc::clone(&handle),
            generation,
            Arc::clone(&self.emitter),
            watcher.clone(),
        ));
        info!(ticket = %ticket_fingerprint, port, paired, "connected to a remote");
        self.emitter.emit(AppEvent::remote_connected(port));
        *live = Some(LiveConnect {
            handle,
            ticket_fingerprint: ticket_fingerprint.clone(),
            generation,
            watcher,
        });
        Ok(Connected {
            port,
            base_url,
            ticket_fingerprint,
            paired,
        })
    }

    /// Close the loopback port; the far machine is unaffected and the stored
    /// pairing stays.
    ///
    /// # Errors
    ///
    /// `Conflict` when not connected.
    pub async fn disconnect(&self) -> Result<(), GuiError> {
        let taken = self.live_connect.lock().await.take();
        let Some(LiveConnect {
            handle, watcher, ..
        }) = taken
        else {
            return Err(GuiError::Conflict("not connected to a remote".to_owned()));
        };
        watcher.cancel();
        if !handle.shutdown_timeout(DRAIN).await {
            warn!("remote connection drain hit its deadline; remaining requests were cut");
        }
        info!("disconnected from the remote");
        self.emitter.emit(AppEvent::remote_disconnected());
        Ok(())
    }

    /// Stop the far daemon, then disconnect. A one-way door: nothing brings
    /// it back except someone at that machine (ADR 0012, decision 7).
    ///
    /// # Errors
    ///
    /// `Conflict` when not connected; `ValidationFailed` when no key is
    /// stored or the far side refuses it; `Unavailable` when the request did
    /// not get through.
    pub async fn kill_remote(&self) -> Result<(), GuiError> {
        let base_url = {
            let live = self.live_connect.lock().await;
            let Some(live) = live.as_ref() else {
                return Err(GuiError::Conflict(
                    "not connected to a remote — `gglib remote connect` first".to_owned(),
                ));
            };
            live.handle.base_url()
        };
        let key = self
            .settings()
            .await?
            .remote_api_key
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                GuiError::ValidationFailed(
                    "this machine holds no key for the remote, so it cannot stop it".to_owned(),
                )
            })?;
        redeem::kill(&base_url, &key).await?;
        // The far side is going away; take this side down before its
        // watcher reports the closed pipe as a surprise.
        self.disconnect().await
    }

    /// The connect side for the status surface.
    pub(super) async fn connect_snapshot(&self) -> Option<ConnectSnapshot> {
        let live = self.live_connect.lock().await;
        live.as_ref().map(|live| ConnectSnapshot {
            port: live.handle.local_addr().port(),
            base_url: live.handle.base_url(),
            ticket_fingerprint: live.ticket_fingerprint.clone(),
            path: live.handle.status().as_str().to_owned(),
        })
    }

    async fn settings(&self) -> Result<gglib_core::Settings, GuiError> {
        self.core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("could not read settings: {e}")))
    }

    /// Persist what a connection taught us: the ticket always, the key when
    /// a code was redeemed for one.
    async fn remember(&self, key: Option<String>, ticket: String) -> Result<(), GuiError> {
        self.core
            .settings()
            .update(SettingsUpdate {
                remote_api_key: key.map(Some),
                remote_last_ticket: Some(Some(ticket)),
                ..SettingsUpdate::default()
            })
            .await
            .map(drop)
            .map_err(|e| GuiError::Internal(format!("could not store the pairing: {e}")))
    }
}

/// A `ConnectError` as the person who typed `connect` needs to hear it.
fn connect_error(e: ConnectError, port: Option<u16>) -> GuiError {
    match e {
        ConnectError::PeerUnreachable => GuiError::Unavailable(
            "the remote machine could not be reached — it may be off, offline, or its ticket \
             replaced by a newer `gglib remote enable` there"
                .to_owned(),
        ),
        ConnectError::Bind(err) => GuiError::Conflict(format!(
            "could not bind 127.0.0.1:{}: {err}",
            port.map_or_else(|| "<free port>".to_owned(), |p| p.to_string())
        )),
        ConnectError::InvalidRelay { url } => {
            GuiError::ValidationFailed(format!("`{url}` is not a relay URL"))
        }
        other => GuiError::Internal(format!("could not connect: {other}")),
    }
}

/// Follow the connection until it closes, then clear it and say so — unless
/// a newer connection has taken its place, or `disconnect` already did.
async fn watch(
    live: Arc<Mutex<Option<LiveConnect>>>,
    handle: Arc<ConnectHandle>,
    generation: u64,
    emitter: Arc<dyn AppEventEmitter>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => return,
            status = handle.status_changed() => {
                info!(path = status.as_str(), "remote connection path changed");
                if status == PipeStatus::Closed {
                    break;
                }
            }
        }
    }
    let mut guard = live.lock().await;
    if guard.as_ref().is_some_and(|l| l.generation == generation) {
        guard.take();
        warn!("the remote connection closed; `gglib remote connect` to dial again");
        emitter.emit(AppEvent::remote_disconnected());
    }
}
