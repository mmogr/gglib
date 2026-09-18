//! The loop guard's log, which outlives the process.
//!
//! One row for every decision the guard took, and one row per day, model,
//! gglib version and mode counting the requests it scanned. Unlike every
//! counter in [`super::defects`], it survives a restart.
//!
//! It exists for one reader. ADR 0011's kill criterion asks whether the
//! guard's trips reach zero across a denominator large enough that one would
//! have been expected, and a denominator that large is more traffic than one
//! daemon run is likely to see. The per-process ledger cannot answer that, and
//! says why it should not try: a defect *rate* is a claim about recent traffic
//! on this build of everything. This is not a rate kept for a loop to react
//! to. Nothing reads it but a person, through `gglib proxy trips`, the
//! daemon's `GET /api/proxy/loop-guard-trips` and the panel under the guard's
//! own setting; every row is dated, so the reader chooses the window; and
//! every row names the gglib version and the guard's mode, so a reading can be
//! kept to one gglib release and one behaviour instead of averaged across
//! them. `note` re-notes every later turn of a stuck conversation, while
//! `refuse` refuses every one — which, for a client with no recovery path,
//! ends the session — so their trip counts are different populations.
//!
//! That meets the objection only in part. The version is the workspace's, so
//! development builds between two releases share it. Nothing here records the
//! llama.cpp build or the model file, and the removed `defect_windows` scoped
//! evidence by the first of those. A reading that spans a llama.cpp upgrade or
//! a re-downloaded model has to be split by date, by hand.
//!
//! # What a trip row is, and is not
//!
//! A row records the guard's **decision** — noted or refused — not what
//! reached the model. Under `note` the request goes on after the decision, and
//! the note can still fail to arrive: a chat template with no branch for the
//! `tool` role drops the last message and the note inside it; a conversation
//! that also exceeds the context budget is refused `context_length_exceeded`;
//! an embedding model, an unknown model or a failed admission refuses the
//! request before it is sent; and an upstream that dies mid-request can fail
//! the retry. Each of those is still a row here. The dashboard's
//! `loop_guard_trips` is bumped only by a forwarded or refused request's
//! snapshot, so under `note` this log can count more than the dashboard does
//! for the same run.
//!
//! The log can also count *less* than happened. A decision the writer cannot
//! queue — a full queue, or a writer already stopped — is dropped while its
//! scan is still counted, so that day reads as fewer trips over the same
//! denominator. A flush the database refuses loses its trips and scans
//! together, and a forced exit loses whatever the writer had not yet flushed.
//! How many were lost reaches only the daemon's log, as a warning; no reading
//! shows it. A zero read from here rules out a trip only as far as those
//! warnings are absent.
//!
//! Only the proxy's pre-dispatch scan writes here. The agent loop runs the same
//! two detectors and records nothing (#1091).
//!
//! # What is stored
//!
//! No conversation text: no message, tool name, argument or tool result. The
//! tool-call signature (`name:hash|…`) and the session id are each kept only as
//! the first 16 hex digits of their SHA-256: stable keys, so a query on the
//! table can tell "the same loop, seventeen times" from seventeen loops; the
//! per-day summary every reader shows carries only the number of distinct
//! sessions. They are correlation keys, not a privacy boundary — anyone
//! holding the data directory can hash a guess and compare. The one
//! client-chosen string kept as given is the model name, bounded to
//! [`MODEL_NAME_LIMIT`] characters.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::defect_counts::LoopGuardTrip;
use crate::settings::LoopGuardMode;

/// How many days the log keeps, and so the widest window a reader can ask for.
pub const LOOP_GUARD_LOG_RETENTION_DAYS: u32 = 90;

/// The window a reader gets when it names none.
pub const LOOP_GUARD_LOG_DEFAULT_DAYS: u32 = 30;

/// The longest model name a row keeps. The client chooses the name, and the
/// proxy sets no request-size limit of its own.
pub const MODEL_NAME_LIMIT: usize = 256;

/// The version every row is stamped with: the workspace's, which is gglib's.
pub const GGLIB_VERSION: &str = env!("CARGO_PKG_VERSION");

const SECS_PER_DAY: u64 = 86_400;

/// The UTC day `secs` falls on, counted from the Unix epoch.
///
/// Trips and scans are both grouped by it, computed from the one timestamp
/// the guard takes per request, so a request scanned a moment before midnight
/// cannot have its trip counted against the next day.
pub fn epoch_day(secs: u64) -> i64 {
    i64::try_from(secs / SECS_PER_DAY).unwrap_or(i64::MAX)
}

/// The first day of a window of `days` days that ends with the day `now_secs`
/// falls on, `days` clamped to 1..=[`LOOP_GUARD_LOG_RETENTION_DAYS`].
pub fn first_day_of_window(now_secs: u64, days: u32) -> i64 {
    let days = days.clamp(1, LOOP_GUARD_LOG_RETENTION_DAYS);
    epoch_day(now_secs) - i64::from(days) + 1
}

/// The model name a row keeps: the client's, cut at [`MODEL_NAME_LIMIT`]
/// characters.
pub fn bounded_model_name(model_name: &str) -> String {
    model_name.chars().take(MODEL_NAME_LIMIT).collect()
}

/// The first 16 hex digits of `text`'s SHA-256.
fn short_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        // Writing to a `String` cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// One decision the loop guard took about one request.
///
/// Its hashed fields have no setter that takes a hash: the only way in is the
/// text itself, through [`Self::with_signature`] and [`Self::with_session`],
/// which keep the hash and drop the text. A row therefore cannot hold a
/// signature or a session id by construction, not by convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopGuardTripEvent {
    recorded_at_secs: u64,
    model_name: String,
    detector: LoopGuardTrip,
    mode: LoopGuardMode,
    signature_hash: Option<String>,
    session_hash: Option<String>,
    repeat_count: Option<u32>,
    threshold: Option<u32>,
}

impl LoopGuardTripEvent {
    /// A decision taken at `recorded_at_secs` by `detector` under `mode`:
    /// [`LoopGuardMode::Note`] decided to forward the request with a note,
    /// [`LoopGuardMode::Refuse`] refused it. Never [`LoopGuardMode::Off`],
    /// which scans nothing and so decides nothing.
    pub fn new(
        recorded_at_secs: u64,
        model_name: &str,
        detector: LoopGuardTrip,
        mode: LoopGuardMode,
    ) -> Self {
        debug_assert!(mode.scans(), "a guard that is off decides nothing");
        Self {
            recorded_at_secs,
            model_name: bounded_model_name(model_name),
            detector,
            mode,
            signature_hash: None,
            session_hash: None,
            repeat_count: None,
            threshold: None,
        }
    }

    /// The repeated tool-call batch's signature, kept only as its hash.
    #[must_use]
    pub fn with_signature(mut self, signature: &str) -> Self {
        self.signature_hash = Some(short_hash(signature));
        self
    }

    /// The request's session id, kept only as its hash.
    #[must_use]
    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_hash = Some(short_hash(session_id));
        self
    }

    /// How many times the reply repeated, and the threshold it crossed.
    #[must_use]
    pub const fn with_repeats(mut self, count: u32, threshold: u32) -> Self {
        self.repeat_count = Some(count);
        self.threshold = Some(threshold);
        self
    }

    /// When the decision was taken, in seconds since the Unix epoch.
    pub const fn recorded_at_secs(&self) -> u64 {
        self.recorded_at_secs
    }

    /// The model the request named, bounded.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Which detector tripped.
    pub const fn detector(&self) -> LoopGuardTrip {
        self.detector
    }

    /// The mode the decision was taken under.
    pub const fn mode(&self) -> LoopGuardMode {
        self.mode
    }

    /// The signature's hash, for a loop.
    pub fn signature_hash(&self) -> Option<&str> {
        self.signature_hash.as_deref()
    }

    /// The session id's hash: the client's `x-gglib-session-id`, or the one
    /// gglib derives from the conversation's opening when there is none.
    pub fn session_hash(&self) -> Option<&str> {
        self.session_hash.as_deref()
    }

    /// How many times the reply repeated, for stagnation.
    pub const fn repeat_count(&self) -> Option<u32> {
        self.repeat_count
    }

    /// The threshold that count crossed, for stagnation.
    pub const fn threshold(&self) -> Option<u32> {
        self.threshold
    }
}

/// One day of the log for one model, gglib version and mode: how many
/// requests the guard scanned, and how many it acted on — ordinarily some of
/// those, though a trip whose scan was lost has none.
///
/// A day the guard scanned but never tripped is here with `trips` at zero.
/// That row is the reading ADR 0011's criterion is about, so the log is never
/// read from the trips alone. A trip whose scan was lost is here too, with
/// `scanned` at zero.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct LoopGuardTripDay {
    /// The UTC day, as `YYYY-MM-DD`.
    pub day: String,
    /// The model the requests named, bounded.
    pub model_name: String,
    /// The gglib version that scanned them.
    pub gglib_version: String,
    /// The mode they were scanned under: `note` or `refuse`.
    pub mode: LoopGuardMode,
    /// Requests the guard scanned — the denominator.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub scanned: u64,
    /// The requests it acted on: noted under `note`, refused under
    /// `refuse`. A decision, not a delivery: a noted request can still fail
    /// to reach the model.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub trips: u64,
    /// Of the trips, the ones [`LoopGuardTrip::Loop`] raised.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub loops: u64,
    /// Of the trips, the ones [`LoopGuardTrip::Stagnation`] raised.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub stagnations: u64,
    /// Distinct session ids among this row's trips — the client's
    /// `x-gglib-session-id`, or the one gglib derives from the conversation's
    /// opening when there is none. Per row: a session that trips on two days is
    /// counted on each, so this does not add across rows.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub sessions: u64,
}

#[cfg(test)]
#[path = "loop_guard_log_tests.rs"]
mod tests;
