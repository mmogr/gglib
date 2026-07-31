//! `Retry-After` header parsing.
//!
//! RFC 9110 §10.2.3 permits two forms, and upstreams use both: `delta-seconds`
//! (`Retry-After: 5`) and an HTTP-date (`Retry-After: Wed, 21 Oct 2015 07:28:00
//! GMT`). Parsing only the first would silently discard a valid hint from any
//! upstream that prefers the second, so both are handled.
//!
//! `now` is a parameter rather than being read from the clock, so the date form
//! is testable without freezing time.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, RETRY_AFTER};

/// Parse `Retry-After` into a delay relative to `now`.
///
/// Returns `None` when the header is absent, non-ASCII, or unparseable in
/// either form — the caller then falls back to its own backoff schedule. A date
/// already in the past yields `Duration::ZERO` rather than `None`: the upstream
/// did express a hint, and its meaning is "you may retry immediately".
pub(super) fn parse_retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();

    // delta-seconds is the common form, so try it first.
    if let Ok(secs) = raw.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }

    // IMF-fixdate — the preferred HTTP-date format, parseable as RFC 2822.
    let deadline = DateTime::parse_from_rfc2822(raw).ok()?.with_timezone(&Utc);
    Some((deadline - now).to_std().unwrap_or(Duration::ZERO))
}

#[cfg(test)]
#[path = "headers_tests.rs"]
mod headers_tests;
