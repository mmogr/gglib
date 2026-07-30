//! `Retry-After` parsing tests.
//!
//! `now` is injected, so the date form is asserted exactly without freezing the
//! clock.

use chrono::TimeZone;
use reqwest::header::{HeaderMap, HeaderValue};

use super::*;

/// A fixed instant to measure HTTP-dates against.
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2015, 10, 21, 7, 28, 0)
        .single()
        .expect("unambiguous test instant")
}

fn headers_with(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_str(value).expect("ascii"));
    headers
}

#[test]
fn delta_seconds_is_taken_literally() {
    let parsed = parse_retry_after(&headers_with("5"), now());
    assert_eq!(parsed, Some(Duration::from_secs(5)));
}

#[test]
fn zero_delta_seconds_means_retry_immediately() {
    let parsed = parse_retry_after(&headers_with("0"), now());
    assert_eq!(parsed, Some(Duration::ZERO));
}

#[test]
fn surrounding_whitespace_is_tolerated() {
    let parsed = parse_retry_after(&headers_with("  7  "), now());
    assert_eq!(parsed, Some(Duration::from_secs(7)));
}

#[test]
fn an_http_date_becomes_a_relative_delay() {
    // Two minutes after the fixed `now`.
    let parsed = parse_retry_after(&headers_with("Wed, 21 Oct 2015 07:30:00 GMT"), now());
    assert_eq!(parsed, Some(Duration::from_secs(120)));
}

#[test]
fn an_http_date_in_the_past_means_retry_immediately() {
    // The upstream did express a hint; it has simply already elapsed. That is
    // different from no hint at all, so it must not collapse to `None`.
    let parsed = parse_retry_after(&headers_with("Wed, 21 Oct 2015 07:00:00 GMT"), now());
    assert_eq!(parsed, Some(Duration::ZERO));
}

#[test]
fn an_absent_header_yields_no_hint() {
    assert_eq!(parse_retry_after(&HeaderMap::new(), now()), None);
}

#[test]
fn an_unparseable_value_yields_no_hint() {
    // The caller then falls back to its own schedule rather than guessing.
    for value in ["soon", "", "-1", "5.5", "Wed, 99 Xxx 2015 07:30:00 GMT"] {
        assert_eq!(
            parse_retry_after(&headers_with(value), now()),
            None,
            "value {value:?} should not parse"
        );
    }
}
