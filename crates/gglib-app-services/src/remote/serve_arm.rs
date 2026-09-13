//! Arming the serve side: everything from the key to the armed pairing,
//! with the slot already reserved.
//!
//! Split from `serve.rs` when that file reached its budget, along the seam
//! `arm`'s own doc already named. `serve.rs` decides whether to arm and holds
//! the reservation; this is the span in which the reservation is held while
//! the lock is not. One function with one caller, so there is still one place
//! that gives the slot back.

use std::net::SocketAddr;
use std::sync::Arc;

use gglib_core::events::AppEvent;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use gglib_runtime::proxy::ProxyStatus;

use super::backend::Backend;
use super::key;
use super::key::identity_path;
use super::pairing::Offer;
use super::rotation::rotation_poll;
use super::serve_switch::CANCELLED_BY_DISABLE;
use super::types::{EnableRequest, Enabled};
use super::{DRAIN, Live, RemoteOps, WAIT_ONLINE};
use super::{device_keys, enrolment, roster};
use crate::error::GuiError;

impl RemoteOps {
    /// Everything from the key to the armed pairing, with the slot already
    /// reserved.
    ///
    /// Split out for the reason `dial` is: it is the span in which the
    /// reservation is held while the lock is not, and one function with one
    /// caller means one place that gives the slot back.
    pub(super) async fn arm(
        &self,
        request: EnableRequest,
        addr: &SocketAddr,
        generation: u64,
        cancel: &CancellationToken,
        proxy_exit: watch::Receiver<ProxyStatus>,
        offer: Offer,
    ) -> Result<Enabled, GuiError> {
        let settled = key::settle(&self.proxy, &self.core).await?;

        let backend = Backend::at(*addr);

        let mut opts = modelpipe::ServeOptions::default();
        // Named, not Supplied: nothing admits but the tokens seeded below and
        // a live grant. `backend_auth` keeps the proxy's own credential off
        // every device — the edge presents it upstream in the device's place,
        // so ADR 0012 decision 2's local lock is untouched.
        opts.auth = modelpipe::TokenPolicy::Named;
        opts.backend_auth = Some(settled.key.clone());
        opts.relay = request.relay;
        opts.identity = identity_path()?;
        opts.port_mapping = false;
        opts.discovery = request.discovery;
        opts.wait_online = Some(WAIT_ONLINE);
        opts.allow_private_backend = backend.allow_private;
        let handle = modelpipe::serve(&backend.url, opts).await.map_err(|e| {
            GuiError::Internal(format!("could not start the remote tunnel: {}", chain(&e)))
        })?;
        let handle = Arc::new(handle);

        // The last moment anything notices a proxy that went away while the
        // tunnel was binding: the watcher that takes over from here is not
        // spawned until the install below.
        super::backend::refuse_if_gone(&self.proxy, &backend, &handle).await?;

        // Read before the commit below, not after. Under `Named` the listener
        // starts closed, so this is what makes the machine reachable at all —
        // and a key file that cannot be parsed is refused rather than
        // softened into an empty roster. Doing that *after* the commit would
        // fail an enable that had already minted and persisted
        // `proxy_api_key`, locking the local proxy on the way out. The seed
        // itself happens under the install guard below; this read is what
        // makes an unreadable file fail while failing is still free.
        let devices = device_keys::read_keys()?;

        // The first and only thing this leaves on the machine, and the last
        // point at which leaving nothing is still free: everything that can
        // fail is above it, and the tunnel above it is undone by dropping the
        // handle — nothing can be in flight behind a ticket that has not left
        // this function.
        settled.commit(&self.core, cancel).await?;

        // The slot is claimed and the session begun under **one** hold of
        // the lock. Claiming first is not enough: dropping the guard wakes
        // whatever `disable` is queued behind it, and on a multi-thread
        // runtime that `disable` runs in parallel with the lines after the
        // guard — taking the tunnel down while this call is still on its
        // way to `begin_session`, which would then start a session nothing
        // owns. Under one guard a `disable` finds either a reservation or a
        // tunnel, never something in between.
        //
        // The pairing code is *not* armed here: `offer` does that after the
        // guard, on the epoch this returns. What keeps that safe is
        // `reset_session_if` retiring the epoch, so a code cannot be armed
        // against a session that ended — a live `PAIRING_TTL` grant on a
        // dead tunnel would be spendable by anything local, since
        // `POST /v1/remote/pair` sits outside the proxy's bearer group.
        //
        // Nothing in here is slow: an install, a `Mutex<Option<_>>`, an
        // atomic, one small file read, and a wait on `roster` that only a
        // concurrent roster write can hold up.
        let watchers = CancellationToken::new();
        let mut slot = self.live.lock().await;
        // Begun before the install so the epoch can go into `Live`, and
        // under the same guard so the pair is still atomic: `reset_session_if`
        // is what undoes it on the one path where the install loses.
        let epoch = self.gateway.begin_session(request.allow_mcp);
        let live = Live {
            handle: Arc::clone(&handle),
            cancel: watchers.clone(),
            epoch,
        };
        if !slot.install(generation, live) {
            // A `disable` took the reservation while this was arming, so the
            // session just begun belongs to nothing. Cleared by epoch rather
            // than outright: a later `enable` may already own the gateway.
            self.gateway.reset_session_if(epoch);
            // Before the drain, which is the slow part this lock may not be
            // held across.
            drop(slot);
            if !handle.shutdown_timeout(DRAIN).await {
                warn!("remote tunnel drain hit its deadline; remaining requests were cut");
            }
            return Err(GuiError::Conflict(CANCELLED_BY_DISABLE.to_owned()));
        }
        // Under the guard, which is what makes a `forget` racing an arm come
        // out right; `device_keys::seed` has the reasoning.
        device_keys::seed(self, &handle, devices).await;
        // The roster's writer, under the same guard as the install. The
        // gateway takes notes on the request path, where it may not await,
        // and this is the other end of that channel — here rather than below
        // because a `disable` landing after the guard is released clears the
        // channel, and this would then put one back for a session that had
        // already ended. Non-blocking: a channel and a spawn.
        roster::start(
            &self.gateway,
            Arc::clone(&self.core),
            Arc::clone(&self.roster),
        );
        drop(slot);
        // Both watchers start only now, and the ordering is load-bearing.
        // `watch_proxy` takes the slot with `take_if`, which looks at a
        // *full* slot and passes over a reservation — so one spawned while
        // `arm` still held the reservation would find nothing, return, and
        // never look again, leaving a live tunnel in front of a dead proxy
        // with nothing following it. `refuse_if_gone` covers the window up
        // to here; from here the watcher does.
        if !settled.pinned {
            tokio::spawn(rotation_poll(
                Arc::clone(&self.core),
                Arc::clone(&handle),
                settled.key,
                watchers.clone(),
            ));
        }
        super::backend::follow_proxy(self, &handle, proxy_exit, watchers, backend);

        let ticket = handle.ticket();
        let fingerprint = ticket.fingerprint();
        info!(ticket = %fingerprint, mcp = request.allow_mcp, "remote tunnel enabled");
        self.emitter.emit(AppEvent::remote_enabled(fingerprint));

        let ticket = ticket.to_string();
        let pairing = match offer {
            Offer::Code => Some(enrolment::offer(self, &handle, epoch, &ticket).await?),
            Offer::Silent => None,
        };
        Ok(Enabled {
            ticket,
            pairing,
            mcp_allowed: request.allow_mcp,
            // This path armed the session, so the request's flags are the
            // session's flags and every one of them took.
            already_up: false,
        })
    }
}

/// An error and everything under it, joined into one sentence.
///
/// modelpipe's `Display` for `ServeError::Identity` says only that the file
/// cannot be used and leaves the reason to its source, on the stated grounds
/// that anyhow prints the chain. Nothing on this path uses anyhow, so
/// formatting with `{e}` alone dropped the half that says what to do about it
/// — "the identity file is readable by others (mode 0644) — chmod 600 it" —
/// and left the operator with a sentence naming a path and no fault.
fn chain(error: &dyn std::error::Error) -> String {
    let mut sentence = error.to_string();
    let mut source = error.source();
    while let Some(next) = source {
        sentence.push_str(": ");
        sentence.push_str(&next.to_string());
        source = next.source();
    }
    sentence
}
