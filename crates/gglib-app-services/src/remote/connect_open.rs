//! Reaching the far machine for a `join`: pairing when the string carries a
//! code, waiting when it does not, and what each refusal says.
//!
//! A child of `connect_dial.rs`, which would cross its size budget with this
//! in it, and a subject of its own: `dial` is the reserved span and what the
//! record and the slot are owed once a pipe exists; this is how the pipe comes
//! to exist, and what `disconnect` may end while it does.

use std::error::Error as _;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use modelpipe::{
    ConnectError, ConnectHandle, PairError, PairingString, PairingStringError, Unreached,
};
use tokio_util::sync::CancellationToken;

use super::super::super::types::ConnectRequest;
use super::super::{DRAIN, cancelled, connect_error};
use crate::error::GuiError;

/// How long a dial with no code waits for the far machine before giving up.
///
/// One full attempt at a machine that is not there: iroh spends about thirty
/// seconds before it stops trying, so a shorter wait would report "did not
/// answer" while the dial was still going.
const FIRST_CONTACT: Duration = Duration::from_secs(30);

/// How long a dial with a code waits for the far machine before giving up.
///
/// Shorter than [`FIRST_CONTACT`], and the reason is arithmetic. The CLI
/// allows `gglib remote join` sixty seconds in all, at
/// `daemon_client/remote.rs`. After this wait `modelpipe::pair` gives the
/// exchange up to thirty seconds of its own, which gglib cannot change, so
/// twenty-five here makes fifty-five for the pairing, and a slow one ends in
/// a sentence saying what happened rather than in the CLI's own timeout. A dial with no
/// code has no exchange after its wait, so it keeps the full thirty.
const REACH_WITHIN: Duration = Duration::from_secs(25);

/// A pairing string as somebody pasted it, or as it was stored.
///
/// Trimmed with `str::trim` first, because modelpipe trims ASCII whitespace
/// only, and a string copied out of a chat app or a PDF can end in a no-break
/// space, which gglib has always let go. A refusal is put as the person who
/// typed it needs to hear it: modelpipe's sentence, the ticket's own reason
/// when that is what failed, and "that is not a ticket" for a string with no
/// code, which modelpipe calls the part before a code.
pub(in crate::remote::connect) fn parse_pairing(typed: &str) -> Result<PairingString, GuiError> {
    let typed = typed.trim();
    typed.parse().map_err(|e: PairingStringError| {
        let said = match e {
            PairingStringError::Ticket(_) if !typed.contains('-') => {
                "that is not a ticket".to_owned()
            }
            _ => e.to_string(),
        };
        GuiError::ValidationFailed(match e.source() {
            Some(cause) => format!("{said}: {cause}"),
            None => said,
        })
    })
}

/// A pipe that has reached the far machine, and the key a code bought when
/// the dial carried one.
pub(super) struct Opened {
    pub(super) handle: ConnectHandle,
    pub(super) key: Option<String>,
}

/// Why [`open`] handed back no pipe.
pub(super) enum NotOpened {
    /// `gglib remote disconnect` gave up on a dial with no code.
    Cancelled,
    /// The bind or the dial failed before anything was reached.
    Connect(ConnectError),
    /// A dial with no code reached nobody.
    Unreached(Unreached),
    /// A dial with a code did not pair.
    Pair(PairError),
}

impl NotOpened {
    /// Whether the port could not be bound, which on a port nobody pinned is
    /// not a failure yet.
    pub(super) const fn is_bind(&self) -> bool {
        matches!(
            self,
            Self::Connect(ConnectError::Bind(_))
                | Self::Pair(PairError::Connect(ConnectError::Bind(_)))
        )
    }

    /// The refusal as the person who typed `gglib remote join` needs to hear
    /// it.
    pub(super) fn into_error(self, port: Option<u16>) -> GuiError {
        match self {
            Self::Cancelled => cancelled(),
            Self::Connect(e) | Self::Pair(PairError::Connect(e)) => connect_error(e, port),
            Self::Unreached(why) | Self::Pair(PairError::Unreached(why)) => unreached(why),
            Self::Pair(PairError::Refused) => GuiError::ValidationFailed(
                "the far machine refused the pairing code — it was mistyped, has expired (two \
                 minutes), or was used already; check it and run `gglib remote join` again while \
                 it is still on screen, or run `gglib remote invite` there once it has gone"
                    .to_owned(),
            ),
            Self::Pair(PairError::Exchange(e)) => GuiError::Unavailable(format!(
                "the pairing request did not get through, and the code may have been spent on \
                 the way: {e}"
            )),
            // What a desktop on gglib 0.18 answers: its edge spends the code
            // and its proxy has no route, so the pairing request arrives at a
            // router that has never heard of it.
            Self::Pair(PairError::UnexpectedStatus { status: 404 }) => GuiError::Unavailable(
                "the far machine answered the pairing request with HTTP 404, which is not a \
                 pairing answer — a desktop on gglib 0.18 or older does, because it pairs \
                 another way: update it, then run `gglib remote invite` there for a new code"
                    .to_owned(),
            ),
            // Any other status is unexplained, and an unexplained status
            // must not be blamed on a version: 404 is the only one anyone has
            // traced to a mechanism, and sending an operator after a desktop
            // that may already be current wastes their time. The code may be
            // spent either way.
            Self::Pair(PairError::UnexpectedStatus { status }) => GuiError::Unavailable(format!(
                "the far machine answered the pairing request with HTTP {status}, which is not a \
                 pairing answer; the code may have been spent, so run `gglib remote invite` there \
                 for a new one"
            )),
            // The arm that carries no status: an answer this side could not
            // read as a pairing answer, which mostly means a 200 whose body
            // was wrong — a field is missing, a body that is not UTF-8, an
            // empty key or device — or no response head at all. An
            // answer that stopped before it was whole is `Exchange`, which
            // this match answers further up, and never reaches here.
            Self::Pair(PairError::Unexpected(why)) => GuiError::Unavailable(format!(
                "the far machine's answer to the pairing request was not a pairing answer \
                 ({why}); the code may have been spent, so run `gglib remote invite` there for a \
                 new one"
            )),
            Self::Pair(other) => GuiError::Internal(format!("could not pair: {other}")),
        }
    }
}

/// A dial that never reached the far machine.
fn unreached(why: Unreached) -> GuiError {
    match why {
        Unreached::TimedOut(within) => GuiError::Unavailable(format!(
            "the remote machine did not answer within {} seconds — it may be off, offline, or \
             have had its endpoint key deleted since; its ticket does not change on its own",
            within.as_secs()
        )),
        // Closed, and whatever modelpipe adds. Not the far machine's doing:
        // before a path forms there is nothing over there to close the pipe,
        // so this is the local listener having stopped under it, and nothing
        // was sent. On a dial with a code, `pair` presents it only after the
        // wait, so the code is unspent too.
        _ => GuiError::Unavailable(
            "the tunnel closed before the remote machine answered — nothing was sent through it"
                .to_owned(),
        ),
    }
}

/// Bind the loopback side on `port`, or on any free one, and reach the far
/// machine: by pairing when the string carries a code, by waiting when it
/// does not.
///
/// **Cancellable only while nothing can have been spent.** Without a code
/// there is nothing to lose, so `disconnect` ends the bind and the wait at
/// once, which is exactly when somebody types it. With one,
/// `modelpipe::pair` connects, waits and presents the code in one call, and
/// nothing here can see which of those it is in: abandoning it could spend
/// the code on a pairing nobody collects, and the recovery for that is a
/// walk to the other machine. So a `disconnect` racing a pairing takes the
/// slot at once and lets the pairing finish, at most [`REACH_WITHIN`] and
/// modelpipe's thirty seconds, before `dial` finds the slot gone and takes
/// the port down with the key already stored.
///
/// Nor can the code be presented before the far machine is reached: `pair`
/// connects, waits, and only then exchanges. That ordering was gglib's to
/// keep while gglib redeemed the code itself, and it is modelpipe's now.
pub(super) async fn open(
    pairing: &PairingString,
    request: &ConnectRequest,
    port: Option<u16>,
    cancel: &CancellationToken,
) -> Result<Opened, NotOpened> {
    let mut opts = modelpipe::ConnectOptions::default();
    opts.bind = port.map(|port| SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    opts.relay = request.relay.clone();
    opts.port_mapping = false;
    opts.discovery = request.discovery;
    if pairing.code().is_some() {
        // No label: `gglib remote join` has never sent the far side a name
        // for this machine, and the row there reads the same as it did.
        let paired = modelpipe::pair(pairing, None, opts, REACH_WITHIN)
            .await
            .map_err(NotOpened::Pair)?;
        return Ok(Opened {
            handle: paired.handle,
            key: Some(paired.api_key),
        });
    }
    let handle = tokio::select! {
        () = cancel.cancelled() => return Err(NotOpened::Cancelled),
        dialled = modelpipe::connect(pairing.ticket(), opts) => dialled.map_err(NotOpened::Connect)?,
    };
    let reached = tokio::select! {
        () = cancel.cancelled() => None,
        reached = handle.wait_reachable(FIRST_CONTACT) => Some(reached),
    };
    match reached {
        Some(Ok(_)) => Ok(Opened { handle, key: None }),
        failed => {
            // `DRAIN`, like every other teardown here. A dial that reached
            // nobody has nothing of its own in flight, but the port it bound
            // has been answering `502` to anything local for as long as the
            // wait lasted, and a third-party client mid-request on it is owed
            // the same five seconds every other path gives one.
            handle.shutdown_timeout(DRAIN).await;
            Err(failed.map_or(NotOpened::Cancelled, |why| {
                NotOpened::Unreached(why.expect_err("the success arm is above"))
            }))
        }
    }
}

#[cfg(test)]
#[path = "connect_open_tests.rs"]
mod connect_open_tests;
