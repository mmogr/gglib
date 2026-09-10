//! The span of `connect` in which the slot is reserved and the lock is not.
//!
//! A second `impl` block carved off `connect.rs` because that file is at
//! its size budget — the same answer `settings_validate.rs` and
//! `stored_pairing.rs` are — and because the span is worth naming: between
//! the reservation and the install, a `disconnect` may take the slot away,
//! and everything here has to be written as though it will.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicI64;

use gglib_core::events::AppEvent;
use gglib_core::{DEFAULT_REMOTE_PORT, RemotePairing};
use modelpipe::{ConnectError, ConnectHandle, Ticket};
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
        let wanted = wanted_port(request.port, held.as_ref());
        let (handle, moved_from) = match bind(ticket, request, Some(wanted), cancel).await? {
            Ok(handle) => (handle, None),
            Err(ConnectError::Bind(_)) if request.port.is_none() => {
                let handle = bind(ticket, request, None, cancel)
                    .await?
                    .map_err(|e| connect_error(e, None))?;
                (handle, Some(wanted))
            }
            Err(e) => return Err(connect_error(e, request.port)),
        };
        let handle = Arc::new(handle);
        let base_url = handle.base_url();
        let port = handle.local_addr().port();

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
                settle(
                    &self.core,
                    ticket,
                    held.as_ref(),
                    code,
                    port,
                    async move |code| redeem::redeem(&reached, over, &code).await,
                )
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

        let ticket_fingerprint = ticket.fingerprint();
        let watcher = CancellationToken::new();
        let away_since = Arc::new(AtomicI64::new(-1));
        let live = LiveConnect {
            handle: Arc::clone(&handle),
            ticket_fingerprint: ticket_fingerprint.clone(),
            generation,
            watcher: watcher.clone(),
            away_since: Arc::clone(&away_since),
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
            away_since,
            port,
        ));
        info!(ticket = %ticket_fingerprint, port, paired, ?moved_from, "connected to a remote");
        self.emitter.emit(AppEvent::remote_connected(port));
        Ok(Connected {
            port,
            base_url,
            ticket_fingerprint,
            paired,
            moved_from,
        })
    }
}

/// The port to try first: `--port` if one was given, else the port this
/// pairing was last reachable on, else the default.
///
/// The address a client was configured against should be the address next
/// time, so the order is *pinned, remembered, default* and never "a free
/// one". Only a port nobody pinned may move, and when it does the move is
/// reported and the new port written back — stable, not fixed.
///
/// A function rather than three lines inside the dial because the dial
/// itself cannot be reached from a test: `modelpipe::connect` wants an iroh
/// endpoint and a peer that answers. The choice is the part with a rule in
/// it, so the choice is what is testable.
fn wanted_port(requested: Option<u16>, held: Option<&RemotePairing>) -> u16 {
    requested
        .or_else(|| held.and_then(|held| held.port))
        .unwrap_or(DEFAULT_REMOTE_PORT)
}

/// Bind the loopback side and start dialling — on `port`, or on any free
/// one — unless `disconnect` gave up on this dial first.
///
/// The outer `Err` is the cancellation; the inner is modelpipe's, left for
/// the caller to read, because a bind that failed on a port nobody pinned
/// is not a failure yet.
async fn bind(
    ticket: &Ticket,
    request: &ConnectRequest,
    port: Option<u16>,
    cancel: &CancellationToken,
) -> Result<Result<ConnectHandle, ConnectError>, GuiError> {
    let mut opts = modelpipe::ConnectOptions::default();
    opts.bind = port.map(|port| SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    opts.relay = request.relay.clone();
    opts.port_mapping = false;
    opts.discovery = request.discovery;
    tokio::select! {
        () = cancel.cancelled() => Err(cancelled()),
        dialled = modelpipe::connect(ticket, opts) => Ok(dialled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(port: Option<u16>) -> RemotePairing {
        RemotePairing {
            ticket: "pipe-whatever".to_owned(),
            api_key: "key".to_owned(),
            default_model: None,
            port,
        }
    }

    /// The port a client was configured against is the port tried first.
    ///
    /// All four cases in one place because they are one rule read in order,
    /// and the interesting one is the last: a pin beats the record, so
    /// `--port` on a pairing that already remembers somewhere else moves it
    /// rather than being ignored.
    #[test]
    fn the_port_tried_first_is_the_pinned_one_then_the_remembered_one() {
        assert_eq!(
            wanted_port(None, None),
            DEFAULT_REMOTE_PORT,
            "a first dial has nothing to go on but the default"
        );
        assert_eq!(
            wanted_port(None, Some(&held(Some(8181)))),
            8181,
            "the port this pairing was last reachable on was not tried first"
        );
        assert_eq!(
            wanted_port(None, Some(&held(None))),
            DEFAULT_REMOTE_PORT,
            "a record from before the port was written down falls back to the default"
        );
        assert_eq!(
            wanted_port(Some(9100), Some(&held(Some(8181)))),
            9100,
            "`--port` is a pin and outranks the record"
        );
    }
}
