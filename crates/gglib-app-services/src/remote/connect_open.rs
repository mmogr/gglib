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
            // What a desktop on gglib 0.18 answers: its edge spends the code and
            // its proxy has no route. Matched on modelpipe 0.6's wording, all
            // `Unexpected` carries; a rewording falls to the arm below.
            Self::Pair(PairError::Unexpected(why @ "a status other than 200 or 401")) => {
                GuiError::Unavailable(format!(
                    "the far machine answered the pairing request with something that is not a \
                     pairing answer ({why}) — a desktop on gglib 0.18 or older does, because it \
                     pairs another way: update it, then run `gglib remote invite` there for a new \
                     code"
                ))
            }
            // Any other non-answer (a stream closed early, another endpoint's
            // answer) is not a version problem, and the code may be spent.
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
mod tests {
    use super::*;
    use crate::test_support_remote::TICKET_A;

    /// The two sentences a dial that reached nobody ends in, which
    /// `docs/remote.md`'s troubleshooting table quotes: the wait names the
    /// budget it was given, so a dial with a code says twenty-five.
    #[test]
    fn a_dial_that_reached_nobody_says_which_way_and_how_long_it_waited() {
        let GuiError::Unavailable(waited) = unreached(Unreached::TimedOut(REACH_WITHIN)) else {
            panic!("a wait that ran out is the machine being unavailable");
        };
        assert!(
            waited.starts_with("the remote machine did not answer within 25 seconds"),
            "{waited}"
        );
        let GuiError::Unavailable(closed) = unreached(Unreached::Closed(None)) else {
            panic!("a pipe that closed first is unavailable too");
        };
        assert!(
            closed.starts_with("the tunnel closed before the remote machine answered"),
            "{closed}"
        );
    }

    /// What a pairing that did not pair says: a refused code is most likely a
    /// mistyped one, which costs only that attempt, and an answer that is not a
    /// pairing answer is what a desktop on gglib 0.18 sends back.
    #[test]
    fn a_pairing_that_did_not_pair_says_what_to_do_next() {
        let GuiError::ValidationFailed(refused) =
            NotOpened::Pair(PairError::Refused).into_error(None)
        else {
            panic!("a refused code is the caller's to fix");
        };
        assert!(
            refused.starts_with("the far machine refused the pairing code — it was mistyped"),
            "{refused}"
        );
        let GuiError::Unavailable(old) =
            NotOpened::Pair(PairError::Unexpected("a status other than 200 or 401"))
                .into_error(None)
        else {
            panic!("an answer that is not a pairing answer leaves the far machine unavailable");
        };
        assert!(old.contains("a desktop on gglib 0.18 or older"), "{old}");
        let GuiError::Unavailable(other) =
            NotOpened::Pair(PairError::Unexpected("an empty key or device")).into_error(None)
        else {
            panic!("any answer that is not a pairing answer leaves the far machine unavailable");
        };
        assert!(
            !other.contains("gglib 0.18") && other.contains("may have been spent"),
            "{other}"
        );
    }

    /// A pasted string is trimmed as `str::trim` trims, so a no-break space a
    /// chat app left at either end is not a refusal; and a string with no code
    /// that is not a ticket is called that, not "the part before the code".
    #[test]
    fn a_pasted_string_loses_any_whitespace_and_a_bad_bare_ticket_is_named_as_one() {
        let pasted = format!("\u{a0}{TICKET_A}-483920\u{3000}");
        let pairing = parse_pairing(&pasted).expect("whitespace at either end is let go");
        assert_eq!(
            pairing.code().map(modelpipe::PairingCode::as_str),
            Some("483920")
        );

        let Err(GuiError::ValidationFailed(bare)) = parse_pairing("pipenotaticket") else {
            panic!("a string that is not a ticket is the caller's to fix");
        };
        assert!(bare.starts_with("that is not a ticket: "), "{bare}");
        let Err(GuiError::ValidationFailed(coded)) = parse_pairing("pipenotaticket-483920") else {
            panic!("and so is one with a code");
        };
        assert!(
            coded.starts_with("the part before the code is not a ticket: "),
            "{coded}"
        );
    }
}
