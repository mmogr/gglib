//! [`HubGate`]: per allowance, the moment it opens again.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::time::Instant;

use super::{HubBucket, HubLimit, MAX_RESET, MIN_CLOSED};

/// Per allowance of the Hub's, the moment it opens again, or nothing while
/// it is open.
///
/// A gate can be shared by whatever asks the Hub from one address, so that
/// a refusal one of them was given keeps the others from asking too. Each
/// answer's limit is noted with [`Self::note`], and the latest one noted
/// decides: one with none left closes its allowance until its reset, and one
/// with some left opens it.
#[derive(Debug, Default)]
pub struct HubGate {
    closed_until: Mutex<[Option<Instant>; 3]>,
}

impl HubGate {
    /// A gate with every allowance open.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Note the limit an answer read at `now` stated.
    ///
    /// An allowance with none left is closed until its reset, held between
    /// [`MIN_CLOSED`] and [`MAX_RESET`] from `now`. That is so on a refusal
    /// and on an answer that was served, since the request after it would be
    /// refused. An allowance with some left is open.
    pub fn note(&self, limit: HubLimit, now: Instant) {
        let reopens =
            (limit.remaining == 0).then(|| now + limit.resets_in.clamp(MIN_CLOSED, MAX_RESET));
        self.closed_until
            .lock()
            .unwrap_or_else(PoisonError::into_inner)[limit.bucket.index()] = reopens;
    }

    /// How long `bucket` stays closed after `now`, or `None` when it is open.
    #[must_use]
    pub fn closed(&self, bucket: HubBucket, now: Instant) -> Option<Duration> {
        let until = self
            .closed_until
            .lock()
            .unwrap_or_else(PoisonError::into_inner)[bucket.index()]?;
        (until > now).then(|| until - now)
    }
}
