//! The span of `connect` in which the slot is reserved and the lock is not.
//!
//! A second `impl` block carved off `connect.rs` because that file is at
//! its size budget — the same answer `settings_validate.rs` and
//! `stored_pairing.rs` are — and because the span is worth naming: between
//! the reservation and the install, a `disconnect` may take the slot away,
//! and everything here has to be written as though it will.

use std::sync::Arc;
use std::sync::atomic::AtomicI64;

use gglib_core::events::AppEvent;
use gglib_core::{DEFAULT_REMOTE_PORT, RemotePairing};
use modelpipe::PairingString;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::super::RemoteOps;
use super::super::connect_watch::watch;
use super::super::stored_pairing::settle;
use super::super::types::{ConnectRequest, Connected};
use super::{DRAIN, LiveConnect, cancelled};
use crate::error::GuiError;

#[path = "connect_open.rs"]
mod open;

pub(super) use open::parse_pairing;
use open::{Opened, open};

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
        pairing: &PairingString,
        held: Option<RemotePairing>,
        request: &ConnectRequest,
        generation: u64,
        cancel: &CancellationToken,
    ) -> Result<Connected, GuiError> {
        let ticket = pairing.ticket();
        let wanted = wanted_port(request.port, held.as_ref());
        let (opened, moved_from) = match open(pairing, request, Some(wanted), cancel).await {
            Ok(opened) => (opened, None),
            Err(refused) if refused.is_bind() && request.port.is_none() => {
                let opened = open(pairing, request, None, cancel)
                    .await
                    .map_err(|refused| refused.into_error(None))?;
                (opened, Some(wanted))
            }
            Err(refused) => return Err(refused.into_error(request.port)),
        };
        let Opened { handle, key } = opened;
        let handle = Arc::new(handle);
        let base_url = handle.base_url();
        let port = handle.local_addr().port();

        // What the record owes this dial is `settle`'s, in
        // `stored_pairing.rs`, and it is there rather than here so that it
        // can be driven: nothing below `modelpipe::connect` is reachable in
        // a test, and every arm of that decision is below it. `pair` spent
        // the code before this runs, so the key it bought stands where
        // `settle` takes a code, and the redemption hands it straight back:
        // `open` returns a key exactly when the string carried a code, and a
        // dial with no code is never asked for one. The one thing that stays
        // here is the port, which no arm may leave bound behind a `dial` that
        // reported a failure.
        let settled = settle(&self.core, ticket, held.as_ref(), key, port, async |key| {
            Ok(key)
        })
        .await;
        let paired = match settled {
            Ok(paired) => paired,
            Err(e) => {
                // `DRAIN`, like every other teardown here. The pipe reached
                // the far machine, but a third-party client may already be
                // mid-request on the port it bound, and is owed the same
                // five seconds every other path gives one.
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
            return Err(overtaken(paired));
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
        self.emitter.emit(AppEvent::remote_joined(port));
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

/// What a `join` says when `disconnect` took its slot before the install.
///
/// A dial with a code runs to completion once `pair` has it, so the pairing
/// can finish after the slot is gone, with the key already stored. Saying
/// only "cancelled" then would send somebody to the other machine for a code
/// this one no longer needs.
fn overtaken(paired: bool) -> GuiError {
    if paired {
        GuiError::Conflict(
            "`gglib remote disconnect` ended this join after its pairing had finished: this \
             machine holds the key now, so `gglib remote join` with no argument connects"
                .to_owned(),
        )
    } else {
        cancelled()
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

    /// A `join` overtaken after it paired says the key is kept, and one
    /// overtaken before it paired says it was cancelled, as it always has.
    #[test]
    fn a_join_overtaken_after_it_paired_says_the_key_is_kept() {
        let GuiError::Conflict(paired) = overtaken(true) else {
            panic!("an overtaken join is a conflict either way");
        };
        assert!(paired.contains("holds the key now"), "{paired}");
        let GuiError::Conflict(unpaired) = overtaken(false) else {
            panic!("an overtaken join is a conflict either way");
        };
        assert!(
            unpaired.contains("cancelled by `gglib remote disconnect`"),
            "{unpaired}"
        );
    }
}
