//! The pairing code: one code, one redemption, two minutes.
//!
//! Pure state behind a `std` mutex, never held across an await. The tunnel
//! edge admits one request bearing the code (modelpipe's
//! `grant_once_bounded`); this is the other half — the proxy's pairing
//! route asks here whether that request's code is the one this session
//! minted, and takes the key it stands for.

use std::num::NonZeroU8;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use gglib_core::access::constant_time_eq;
use gglib_core::ports::PairingOutcome;

/// How long a code lives unused.
pub(crate) const PAIRING_TTL: Duration = Duration::from_secs(120);

/// How many wrong codes burn the pairing, at each door that counts them.
///
/// Three is enough to forgive a mistyped digit and too few to guess with.
/// Two counters enforce it and neither sees the other's attempts. This one
/// counts redemptions that reach the proxy, which a local process can spend
/// without holding a ticket — so the ticket is not part of the arithmetic
/// here, as ADR 0012's 2026-09-07 correction records. The tunnel edge counts
/// the wrong bearers that never reach the proxy at all, taking the same
/// bound as [`MAX_ATTEMPTS_AT_EDGE`].
pub(crate) const MAX_ATTEMPTS: u8 = 3;

/// [`MAX_ATTEMPTS`] in the shape modelpipe's edge takes it.
///
/// Converted once rather than written twice: two literals that could drift
/// apart would be two different promises about the same code.
pub(crate) const MAX_ATTEMPTS_AT_EDGE: NonZeroU8 = match NonZeroU8::new(MAX_ATTEMPTS) {
    Some(bound) => bound,
    None => panic!("MAX_ATTEMPTS is not zero"),
};

/// Whether an arming offers a pairing code at all.
///
/// A person running `gglib remote enable` is watching for one. A daemon
/// putting the tunnel back at startup is not, and a code nobody is watching
/// for is a live grant nobody spends — for the two minutes it takes to
/// expire, on a route outside the proxy's bearer group, at every boot. The
/// distinction is a type rather than a `bool` so that the silent path cannot
/// be reached by forgetting an argument.
#[derive(Clone, Copy)]
pub(crate) enum Offer {
    /// Mint a code and grant it once at the edge.
    Code,
    /// Arm the tunnel and nothing else.
    Silent,
}

struct Pending {
    code: String,
    key: String,
    /// The name the edge holds `key` under, handed back with it so the
    /// device learns what it is called here.
    device: String,
    expires: Instant,
    attempts: u8,
}

/// The code a session is currently prepared to redeem, if any.
#[derive(Default)]
pub(crate) struct Pairing {
    pending: Mutex<Option<Pending>>,
}

impl Pairing {
    /// Arm a pairing: `code` redeems for `device`'s `key` until `ttl` passes.
    ///
    /// Replaces any pairing already armed. There is one code at a time, and
    /// the caller decides whether replacing one is allowed — see
    /// [`RemoteGateway::offer_pairing`](super::gateway::RemoteGateway::offer_pairing),
    /// which refuses rather than arming over a live one.
    pub(crate) fn begin_for(&self, code: String, key: String, device: String, ttl: Duration) {
        *self.lock() = Some(Pending {
            code,
            key,
            device,
            expires: Instant::now() + ttl,
            attempts: 0,
        });
    }

    /// Forget a pairing that was armed but never redeemed, and say which
    /// device it was for so its key can be retired.
    pub(crate) fn withdraw(&self) -> Option<String> {
        self.lock().take().map(|pending| pending.device)
    }

    /// Present a code. Exactly one presentation can ever be `Granted`.
    ///
    /// Expiry is checked first, so a code that timed out is dead whatever is
    /// presented. A wrong code counts against the attempts and the third
    /// burns the pairing. A right code is spent by the act of matching.
    pub(crate) fn redeem(&self, presented: &str) -> PairingOutcome {
        let mut slot = self.lock();
        let Some(pending) = slot.as_mut() else {
            return PairingOutcome::Rejected;
        };
        if Instant::now() >= pending.expires {
            *slot = None;
            return PairingOutcome::Rejected;
        }
        if constant_time_eq(pending.code.as_bytes(), presented.as_bytes()) {
            let key = pending.key.clone();
            let device = pending.device.clone();
            *slot = None;
            return PairingOutcome::Granted { key, device };
        }
        pending.attempts += 1;
        if pending.attempts >= MAX_ATTEMPTS {
            *slot = None;
        }
        PairingOutcome::Rejected
    }

    /// Whether a code is currently redeemable.
    pub(crate) fn active(&self) -> bool {
        let mut slot = self.lock();
        match slot.as_ref() {
            Some(pending) if Instant::now() < pending.expires => true,
            Some(_) => {
                *slot = None;
                false
            }
            None => false,
        }
    }

    /// Forget any pairing. `disable` calls this so a code shown for a
    /// session that ended cannot outlive it.
    pub(crate) fn clear(&self) {
        *self.lock() = None;
    }

    // Nothing panics while holding the lock; recovering the guard is the
    // honest answer to an impossible poison.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Pending>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod pairing_tests;
