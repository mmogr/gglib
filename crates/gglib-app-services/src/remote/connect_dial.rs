//! The span of `connect` in which the slot is reserved and the lock is not.
//!
//! A second `impl` block carved off `connect.rs` because that file is at
//! its size budget — the same answer `settings_validate.rs` and
//! `stored_pairing.rs` are — and because the span is worth naming: between
//! the reservation and the install, a `disconnect` may take the slot away,
//! and everything here has to be written as though it will.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use gglib_core::RemotePairing;
use gglib_core::events::AppEvent;
use modelpipe::Ticket;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::super::connect_watch::watch;
use super::super::first_contact::{once_reached, wait};
use super::super::stored_pairing::settle;
use super::super::types::{ConnectRequest, Connected};
use super::super::{RemoteOps, redeem};
use super::{DRAIN, LiveConnect, cancelled, connect_error};
use crate::error::GuiError;

impl RemoteOps {
    /// Everything from the dial to the install, with the slot already
    /// reserved.
    ///
    /// Split out for one reason: it is the span in which the reservation is
    /// held while the lock is not, and every way out of it has to give the
    /// slot back. One function with one caller means one place that does
    /// that, rather than a release on each of six error paths and a seventh
    /// added later without one.
    pub(super) async fn dial(
        &self,
        ticket: &Ticket,
        code: Option<String>,
        held: Option<RemotePairing>,
        request: &ConnectRequest,
        generation: u64,
        cancel: &CancellationToken,
    ) -> Result<Connected, GuiError> {
        let mut opts = modelpipe::ConnectOptions::default();
        opts.bind = request
            .port
            .map(|port| SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
        opts.relay = request.relay.clone();
        opts.port_mapping = false;
        opts.discovery = request.discovery;
        let handle = tokio::select! {
            () = cancel.cancelled() => return Err(cancelled()),
            dialled = modelpipe::connect(ticket, opts) => Arc::new(
                dialled.map_err(|e| connect_error(e, request.port))?,
            ),
        };
        let base_url = handle.base_url();

        // Reach the far machine *before* redeeming anything through it.
        // `modelpipe::connect` hands back a bound port and dials behind it,
        // so up to here nothing has been in touch with the other end: a
        // redeem sent now would spend the one-time code on the `502` the
        // edge answers while there is no peer, and a pipe that had reached
        // nobody would install and read as Connected for the ninety seconds
        // `connect_watch` allows an idle one. `once_reached` keeps the two
        // in that order by handing the redeem a `Reached` only the waiting
        // can mint, rather than by being written above it —
        // `first_contact.rs` carries that argument in full.
        //
        // Cancellable, unlike what follows it. Waiting up to thirty seconds
        // is exactly when somebody types `gglib remote disconnect`, and
        // giving up costs nothing while nothing has been spent.
        //
        // The redeem, from there, is deliberately *not* cancellable: one
        // abandoned half way still burns the code, and the recovery for
        // that costs a walk to the other machine — so a `disconnect` racing
        // it waits the twenty seconds the request is bounded by and takes
        // the connection down afterwards, with the key safely stored.
        //
        // What the record owes this dial is `settle`'s, in
        // `stored_pairing.rs`, and it is there rather than here so that it
        // can be driven: nothing below `modelpipe::connect` is reachable in
        // a test, and every arm of that decision is below it. The one thing
        // that stays here is the port, which no arm may leave bound behind
        // a `dial` that reported a failure.
        let paired = match once_reached(
            wait(handle.status(), || handle.status_changed(), cancel),
            |reached| {
                let over = base_url.as_str();
                settle(&self.core, ticket, held.as_ref(), code, async move |code| {
                    redeem::redeem(&reached, over, &code).await
                })
            },
        )
        .await
        {
            Ok(paired) => paired,
            Err(e) => {
                // `DRAIN`, like every other teardown here. A dial that reached
                // nobody has nothing of its own in flight, but the port it
                // bound has been answering `502` to anything local for as long
                // as the gate waited, and a third-party client mid-request on
                // it is owed the same five seconds every other path gives one.
                handle.shutdown_timeout(DRAIN).await;
                return Err(e);
            }
        };

        let port = handle.local_addr().port();
        let ticket_fingerprint = ticket.fingerprint();
        let watcher = CancellationToken::new();
        let live = LiveConnect {
            handle: Arc::clone(&handle),
            ticket_fingerprint: ticket_fingerprint.clone(),
            generation,
            watcher: watcher.clone(),
        };
        if !self.live_connect.lock().await.install(generation, live) {
            // A `disconnect` took the slot while this was dialling. The
            // port this built is nobody's, so it goes down here rather than
            // staying bound behind a command that reported success.
            handle.shutdown_timeout(DRAIN).await;
            return Err(cancelled());
        }
        tokio::spawn(watch(
            Arc::clone(&self.live_connect),
            handle,
            generation,
            Arc::clone(&self.emitter),
            watcher,
        ));
        info!(ticket = %ticket_fingerprint, port, paired, "connected to a remote");
        self.emitter.emit(AppEvent::remote_connected(port));
        Ok(Connected {
            port,
            base_url,
            ticket_fingerprint,
            paired,
        })
    }
}
