//! Starting, replacing and ending a gateway session.
//!
//! A child of `gateway.rs` rather than a sibling, for two reasons. The state
//! these four methods move is private to [`RemoteGateway`] and should stay
//! that way — a sibling module would need every field opened to the whole of
//! `remote`. And they are one subject: every one of them is the same three
//! lines of reasoning about an epoch.
//!
//! **What the epoch is for.** `enable`, `invite` and the teardown all release
//! the serve slot before they finish — arming takes fifteen seconds and a
//! drain takes five, and holding the lock across either would stop
//! `gglib remote status` answering for that long. So they overlap: a
//! `disable` can be draining while a fresh `enable` arms, and an `invite` can
//! be minting a key for a session that ends before it comes back. Each of
//! these methods is therefore handed the epoch its caller saw, reads the
//! current one under the same lock it acts under, and does nothing when they
//! differ.
//!
//! What that buys is the absence of two specific failures, both of which
//! reach a person: a pairing code left live for its full TTL on a session
//! with no listener behind it — and `POST /v1/remote/pair` sits outside the
//! proxy's bearer group, so anything local could spend it — and a fresh
//! session silently reset by the teardown of the one it replaced, which
//! hands the operator a string that answers `Rejected`.

use std::sync::atomic::Ordering;
use std::time::Duration;

use super::RemoteGateway;

/// What [`RemoteGateway::offer_pairing`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::remote) enum Offered {
    /// Armed: the code redeems for the key it was given.
    Armed,
    /// A later session owns the gateway; nothing was armed.
    Superseded,
    /// A pairing is already open on this session; nothing was armed.
    AlreadyOpen,
}

impl RemoteGateway {
    /// Arm a session — `code` redeems for `key` for `ttl`, and `/mcp` is
    /// open to tunnelled requests or it is not — and say which session that
    /// is. The number comes back for [`Self::reset_session_if`].
    ///
    /// The paired flag is cleared here as well as there, because a teardown
    /// is not guaranteed to run: the session this replaces may still be
    /// draining, and its teardown will decline to touch anything (that is
    /// what the epoch is for). Nobody has paired with a session that is only
    /// now being armed, and `status` would otherwise report the last one's
    /// answer.
    ///
    /// No pairing is armed here. `enable` starts a session; `invite` is what
    /// offers a code, and it does so against a session that is already up —
    /// so any pairing the previous session left is cleared rather than
    /// inherited, and a restart that puts the tunnel back arms nothing at
    /// all. A code nobody is watching for is a live grant nobody spends, on
    /// a route that sits outside the proxy's bearer group.
    pub(in crate::remote) fn begin_session(&self, allow_mcp: bool) -> u64 {
        let mut session = self.session();
        *session += 1;
        self.pairing.clear();
        self.mcp_allowed.store(allow_mcp, Ordering::Relaxed);
        self.paired.store(false, Ordering::Relaxed);
        *session
    }

    /// Offer `code` in exchange for `device`'s `key`, on the session `epoch`
    /// names.
    ///
    /// Epoch-guarded, for [`Self::reset_session_if`]'s reason from the other
    /// side: `invite` reads the live slot, releases it, mints a key and
    /// writes it, then comes back. A `disable` in that window leaves `epoch`
    /// naming a session that is gone, and a code armed on it would stay
    /// redeemable for `PAIRING_TTL` with no listener behind it — reachable,
    /// because `POST /v1/remote/pair` is outside the bearer group.
    ///
    /// Read and act under the one lock, so an `enable` landing between the
    /// check and the arm cannot be armed over.
    pub(in crate::remote) fn offer_pairing(
        &self,
        epoch: u64,
        code: String,
        key: String,
        device: String,
        ttl: Duration,
    ) -> Offered {
        let session = self.session();
        if *session != epoch {
            return Offered::Superseded;
        }
        if self.pairing.active() {
            return Offered::AlreadyOpen;
        }
        self.pairing.begin_for(code, key, device, ttl);
        Offered::Armed
    }

    /// Clear an open invite when it belongs to `device`, and say so.
    ///
    /// No epoch here, and that is the difference from
    /// [`withdraw_pairing`](Self::withdraw_pairing): the caller is `forget`,
    /// which works with the tunnel down and holds no session of its own. What
    /// it knows is a device id, and an invite for a device that is being
    /// retired is one nobody should be able to spend — a code redeemed after
    /// the edge stopped holding its key hands the joining machine a
    /// credential that admits nowhere.
    pub(in crate::remote) fn withdraw_pairing_for(&self, device: &str) -> Option<String> {
        let _session = self.session();
        self.pairing.withdraw_if(device)
    }

    /// Clear the pairing session `epoch` owns and say which device it was
    /// for, so the caller can retire a key nobody will now fetch.
    pub(in crate::remote) fn withdraw_pairing(&self, epoch: u64) -> Option<String> {
        let session = self.session();
        if *session != epoch {
            return None;
        }
        self.pairing.withdraw()
    }

    /// Reset everything session `epoch` owns — the pairing, the `/mcp`
    /// grant, the paired flag, and the roster channel — unless a later
    /// session has taken the gateway over since. The request counters are
    /// history and stay.
    ///
    /// The guard is not defensive: a teardown takes its time. `take_down`
    /// drains for up to `DRAIN` before it gets here, and neither of its
    /// callers holds the `live` lock while it does — holding it would block
    /// `status` for the whole drain. So a `disable` and a fresh `enable` can
    /// overlap, and a teardown that cleared unconditionally would wipe the
    /// session that replaced it: the operator would be handed a pairing
    /// string that answers `Rejected` — "expired, used already, or burned by
    /// wrong attempts", none of it true — and an `/mcp` grant revoked
    /// without a word.
    ///
    /// Read and act under the one lock, which is the reason the epoch is not
    /// a bare atomic: an `enable` landing between a load and the clears
    /// would be wiped by a teardown that had just decided to leave it alone.
    ///
    /// **The epoch is counted up here too, not only in
    /// [`begin_session`](Self::begin_session).** Without that, an epoch whose
    /// session ended with no successor still matches, and every guard in this
    /// file that asks "is this still my session?" answers yes for a session
    /// that is gone. The reachable case is an `invite`: it reads the epoch
    /// under the serve slot, releases it, mints a key and writes two stores —
    /// and a `disable` landing in that window would leave `offer_pairing`
    /// arming a live two-minute code against a tunnel that is down, on a
    /// route that sits outside the proxy's bearer group. Ending a session has
    /// to retire its name along with its state.
    pub(in crate::remote) fn reset_session_if(&self, epoch: u64) {
        let mut session = self.session();
        if *session != epoch {
            return;
        }
        *session += 1;
        self.pairing.clear();
        self.mcp_allowed.store(false, Ordering::Relaxed);
        self.paired.store(false, Ordering::Relaxed);
        // Dropping the sender is what ends `roster_sync`: it reads until the
        // channel closes, so whatever the drain above put in the queue is
        // written first. Here rather than in `take_down` for the same reason
        // as everything else in this function — a superseded teardown would
        // otherwise silence the writer belonging to the session that
        // replaced it, and every label and `last_seen` after that would be
        // dropped on the floor with nothing to say so.
        self.notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}
