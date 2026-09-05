//! The pairing code: one code, one redemption, two minutes.
//!
//! Pure state behind a `std` mutex, never held across an await. The tunnel
//! edge admits one request bearing the code (modelpipe's `grant_once`);
//! this is the other half — the proxy's pairing route asks here whether
//! that request's code is the one this session minted, and takes the key
//! it stands for.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use gglib_core::access::constant_time_eq;
use gglib_core::ports::PairingOutcome;

/// How long a code lives unused.
pub(crate) const PAIRING_TTL: Duration = Duration::from_secs(120);

/// How many wrong codes burn the pairing.
///
/// Three is enough to forgive a mistyped digit and too few to guess with:
/// twenty bits of code, three tries, two minutes, and the ticket required
/// to reach the route at all. ADR 0012 has the arithmetic.
pub(crate) const MAX_ATTEMPTS: u8 = 3;

struct Pending {
    code: String,
    key: String,
    expires: Instant,
    attempts: u8,
}

/// The code a session is currently prepared to redeem, if any.
#[derive(Default)]
pub(crate) struct Pairing {
    pending: Mutex<Option<Pending>>,
}

impl Pairing {
    /// Arm a pairing: `code` redeems for `key` until `ttl` passes.
    ///
    /// Replaces any pairing already armed — there is one code per session,
    /// and re-arming is how `enable` after `disable` starts clean.
    pub(crate) fn begin(&self, code: String, key: String, ttl: Duration) {
        *self.lock() = Some(Pending {
            code,
            key,
            expires: Instant::now() + ttl,
            attempts: 0,
        });
    }

    /// The key a live pairing would hand out has changed — a rotation landed
    /// while the code was still on the screen. Hand out the new one.
    pub(crate) fn update_key(&self, key: String) {
        if let Some(pending) = self.lock().as_mut() {
            pending.key = key;
        }
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
            *slot = None;
            return PairingOutcome::Granted(key);
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
