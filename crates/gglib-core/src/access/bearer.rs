//! Matching an `Authorization` header against the configured bearer token.
//!
//! Split from `mod.rs` rather than added to it because that file is near its
//! complexity budget, and because this is one self-contained decision: given
//! what a client sent and what the endpoint expects, does the request get in.

use super::constant_time_eq;

/// The auth scheme this endpoint speaks, compared case-insensitively.
const BEARER: &str = "bearer";

/// Whether `presented` — the raw `Authorization` header value, or `None` when
/// the client sent none — carries `expected_key`.
///
/// # Why the scheme is matched case-insensitively
///
/// RFC 9110 §11.1 defines the auth scheme as a `token`, and tokens are
/// case-insensitive. `bearer sk-…` is therefore a correct request, and the
/// previous comparison — the whole `"Bearer <key>"` string, byte for byte —
/// answered it with a 401 that said the key was wrong. It was not; only its
/// capitalisation was, and nothing in the response said so. That is the least
/// actionable rejection available, and it matters here beyond pedantry: the
/// tunnel in front of this endpoint accepts the same header the RFC does, so
/// two doors checking one credential disagreed about whether it was valid.
///
/// The scheme comparison is deliberately **not** constant-time. It is a public
/// protocol keyword, not a secret, and there is nothing to leak by returning
/// early on it. Only the credential goes to [`constant_time_eq`].
///
/// # What is still refused
///
/// Everything a lenient comparison would wave through. A different scheme
/// (`Basic <key>`), the bare key with no scheme at all, a prefix of the key,
/// and an empty credential are all rejected. The last of those is load-bearing:
/// settings validation refuses a blank `proxy_api_key` precisely so that
/// `Bearer ` cannot become a credential everyone holds, and splitting the
/// header on its space must not reintroduce that from the other side.
///
/// Whitespace follows the grammar rather than being trimmed indiscriminately:
/// the scheme and the credential are separated by one or more spaces
/// (`1*SP`), and trailing optional whitespace is not part of the credential.
#[must_use]
pub fn bearer_matches(presented: Option<&str>, expected_key: &str) -> bool {
    // A blank expectation can never be satisfied. Unreachable through the
    // settings path, which rejects one, but this function is the last place
    // that assumption could go wrong quietly rather than loudly.
    if expected_key.is_empty() {
        return false;
    }

    let Some(header) = presented else {
        return false;
    };

    // No space means no credential — `"Bearer"` alone, or a bare key sent with
    // the scheme omitted, both land here.
    let Some((scheme, rest)) = header.split_once(' ') else {
        return false;
    };

    if !scheme.eq_ignore_ascii_case(BEARER) {
        return false;
    }

    let credential = rest.trim();
    if credential.is_empty() {
        return false;
    }

    constant_time_eq(credential.as_bytes(), expected_key.as_bytes())
}

#[cfg(test)]
#[path = "bearer_tests.rs"]
mod bearer_tests;
