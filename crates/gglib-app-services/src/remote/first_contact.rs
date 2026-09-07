//! Waiting for the far machine to answer, before anything is spent on it.
//!
//! From modelpipe 0.3.0 [`modelpipe::connect`] returns as soon as the local
//! port is bound: the dial runs behind the handle, and holding a handle says
//! nothing at all about the machine at the other end. Two things go wrong
//! when nothing waits for it.
//!
//! The pairing code is one-time. Redeeming it through a pipe that reaches
//! nobody spends it on the `502` the edge answers while there is no peer,
//! and minting another is a walk to the other machine — so this is what
//! keeps a sleeping desktop from costing that.
//!
//! And [`super::connect_watch`] cannot tell "never reached" from "went
//! away", because both are [`PipeStatus::Idle`]. Without a gate a dial that
//! connected to nothing would install, `gglib remote status` would say
//! Connected for the ninety seconds of that file's grace, and the pipe would
//! then be torn down as though it had died — announcing the loss of a
//! connection that never existed.
//!
//! Written over statuses rather than over a [`modelpipe::ConnectHandle`],
//! for the reason `connect_watch::follow` is: the timing *is* the
//! policy, and a policy nothing can drive is a comment with a timer attached.

use std::future::Future;
use std::time::Duration;

use modelpipe::PipeStatus;
// `tokio::time`'s clock, so a paused test can advance the budget rather than
// wait it out — the same reason `connect_watch` gives for the same choice.
use tokio_util::sync::CancellationToken;

use crate::error::GuiError;

/// How long a fresh dial waits for the far machine before giving up on it.
///
/// One full attempt at a machine that is not there. iroh spends about thirty
/// seconds before it stops trying, so a shorter budget would report "did not
/// answer" while the dial was still going. It also has to leave room inside
/// what the CLI allows the whole command — sixty seconds, at
/// `daemon_client/remote.rs` — for the twenty [`super::redeem`] may take
/// *after* this, which puts the pair at fifty and leaves ten spare.
const FIRST_CONTACT: Duration = Duration::from_secs(30);

/// Proof that the far machine answered.
///
/// Minted only by [`wait`], and demanded by
/// [`redeem`](super::redeem::redeem): the one-time code cannot be spent
/// before contact was made, because the call does not typecheck without a
/// value only the waiting produces.
///
/// **This is the ordering guarantee, and it is deliberately not a test.**
/// The hazard here is not that somebody deletes the gate — a test catches
/// that — it is that somebody moves the redeem in front of it, which leaves
/// the gate called, leaves every name in use, and restores the whole defect.
/// Nothing this side can observe tells the two apart: the difference is one
/// HTTP request to a port whose server is modelpipe's, and it fails either
/// way. So the order is carried by the type instead, where a build checks
/// it rather than a run.
pub(super) struct Reached(());

/// Why a dial never got to use the pipe it bound.
///
/// No `Made` variant: reaching the machine is the unremarkable case and is
/// [`Ok`]. Naming only the failures is what makes [`refusal`] total over
/// them, rather than carrying an arm for a success that cannot arrive.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum NoContact {
    /// The pipe ended before a path formed.
    Closed,
    /// Nothing was heard within [`FIRST_CONTACT`].
    Never,
    /// `gglib remote disconnect` gave up on this dial while it waited.
    Cancelled,
}

/// Wait for the far machine to answer, given where the pipe starts and a way
/// to ask for its next status.
pub(super) async fn wait<F, Fut>(
    initial: PipeStatus,
    next: F,
    cancel: &CancellationToken,
) -> Result<Reached, NoContact>
where
    F: Fn() -> Fut,
    Fut: Future<Output = PipeStatus>,
{
    // Read before waiting, for `follow`'s reason: `status_changed`
    // snapshots at the moment it is polled, so a pipe that reached the peer
    // between the bind and the first call has nothing left to report and
    // this would wait out the whole budget on a live connection.
    if let Some(settled) = settled(initial) {
        return settled;
    }
    let deadline = tokio::time::sleep(FIRST_CONTACT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = cancel.cancelled() => return Err(NoContact::Cancelled),
            () = &mut deadline => return Err(NoContact::Never),
            status = next() => {
                if let Some(settled) = settled(status) {
                    return settled;
                }
            }
        }
    }
}

/// What `status` settles about first contact, or `None` while it is still an
/// open question.
///
/// [`PipeStatus::Idle`] is the open question: it is what the connect side
/// publishes both before it has ever reached the peer and while it is
/// looking for one that went away, and here it can only ever be the first.
/// A variant modelpipe adds later is read the same way, deliberately — the
/// conservative answer to a status this build cannot read is "keep
/// waiting", which costs the budget, where the other answer would install a
/// pipe on the strength of a word it does not understand.
const fn settled(status: PipeStatus) -> Option<Result<Reached, NoContact>> {
    match status {
        PipeStatus::Direct | PipeStatus::Relayed => Some(Ok(Reached(()))),
        PipeStatus::Closed => Some(Err(NoContact::Closed)),
        _ => None,
    }
}

/// Do the thing a dial pays for — once the far machine has answered, and not
/// at all if it has not.
///
/// The order is the whole point, so it is structure rather than two
/// statements somebody could swap. `paid` is handed the [`Reached`] the
/// waiting produced and is not called at all without one — so a redeem
/// cannot be hoisted above this, and a `waiting` that ends in [`NoContact`]
/// costs nothing. What it costs otherwise is the far machine's one-time
/// code, and a code spent into a pipe that reached nobody is a walk to that
/// machine to mint another one.
pub(super) async fn once_reached<W, P, Fut, T>(waiting: W, paid: P) -> Result<T, GuiError>
where
    W: Future<Output = Result<Reached, NoContact>>,
    P: FnOnce(Reached) -> Fut,
    Fut: Future<Output = Result<T, GuiError>>,
{
    match waiting.await {
        Ok(reached) => paid(reached).await,
        Err(no) => Err(refusal(&no)),
    }
}

/// A dial that never reached the far machine, as the person who typed
/// `gglib remote connect` needs to hear it.
fn refusal(no: &NoContact) -> GuiError {
    match no {
        // The sentence `connect_error` used to print for
        // `ConnectError::PeerUnreachable`, moved to the one place that can
        // still mean it: at modelpipe 0.3.0 an absent peer is not an error
        // from `connect` at all, it is a handle that keeps trying.
        NoContact::Never => GuiError::Unavailable(format!(
            "the remote machine did not answer within {} seconds — it may be off, offline, or \
             its ticket replaced by a newer `gglib remote enable` there",
            FIRST_CONTACT.as_secs()
        )),
        // Not the far machine's doing: before a path forms there is nothing
        // over there to close the pipe, so this is the local listener
        // having stopped under it.
        NoContact::Closed => GuiError::Unavailable(
            "the tunnel closed before the remote machine answered — nothing was sent through it"
                .to_owned(),
        ),
        NoContact::Cancelled => super::connect::cancelled(),
    }
}

#[cfg(test)]
#[path = "first_contact_tests.rs"]
mod first_contact_tests;
