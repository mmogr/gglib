//! Fetching and parsing of llama.cpp's native `GET /slots` endpoint.
//!
//! This module is deliberately narrow: it only fetches the endpoint and
//! turns the response into a [`SlotsPollResult`]. It does **not** own a
//! polling loop, backoff state, or a cache of the latest result — see
//! [`crate::slots_poller`] for that.
//!
//! ## Why a strongly-typed struct, not `serde_json::Value`
//!
//! The `/slots` JSON shape is not stable across llama.cpp versions (older
//! builds exposed a flat `n_past` field; the current upstream schema
//! nests generation-progress info under `next_token` and drops `n_past`
//! entirely). Rather than accepting an untyped [`serde_json::Value`] and
//! probing it ad hoc at every call site, [`SlotSnapshot`] declares only the
//! handful of fields the dashboard actually needs, with `Option<T>` (plus
//! `#[serde(default)]`) on every field whose presence varies by version.
//! Fields we don't care about (`params`, `speculative`, etc.) are simply
//! never named, so serde silently drops them — no `deny_unknown_fields`,
//! no brittle JSON-pointer probing, no risk of a partially-unknown schema
//! causing the whole response to fail to parse.

use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

/// Per-request timeout for `GET /slots` polls.
///
/// Deliberately short and independent of the main proxy client's
/// (intentionally unbounded) chat-completion timeout — a hung or
/// struggling llama-server should never make the dashboard poller wait
/// longer than this before the caller treats it as unreachable.
const SLOTS_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

// =============================================================================
// SlotSnapshot
// =============================================================================

/// One llama-server processing slot, as reported by `GET /slots`.
///
/// Only the fields the proxy dashboard needs are modeled here. See the
/// module-level documentation for why `Option<T>` + `#[serde(default)]` is
/// used throughout instead of a raw [`serde_json::Value`].
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SlotSnapshot {
    /// Slot index within the running llama-server (`"id"` in the response).
    pub id: i64,
    /// ID of the task currently occupying this slot. Absent/`-1` means idle,
    /// depending on llama.cpp version; kept as-is (no normalization).
    #[serde(default)]
    pub id_task: Option<i64>,
    /// Context size configured for this slot, in tokens.
    #[serde(default)]
    pub n_ctx: Option<u64>,
    /// Whether this slot is actively processing a request right now.
    #[serde(default)]
    pub is_processing: bool,
    /// Legacy field (older llama.cpp versions): tokens already resident in
    /// this slot's KV cache. Superseded by `next_token` in current upstream
    /// versions, where it is simply absent — kept only as a best-effort
    /// fallback, see [`Self::tokens_in_use`].
    #[serde(default)]
    n_past: Option<u64>,
    /// Alternate legacy field name seen in some intermediate llama.cpp
    /// builds; same role as `n_past`.
    #[serde(default)]
    cache_tokens: Option<u64>,
    /// Current-schema nested object carrying generation progress. We only
    /// need `n_decoded` out of it, as a last-resort usage signal.
    #[serde(default)]
    next_token: Option<NextTokenInfo>,
}

/// The subset of the current schema's `next_token` object we care about.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct NextTokenInfo {
    /// Number of tokens decoded (generated) so far for the active task.
    #[serde(default)]
    n_decoded: Option<u64>,
}

impl SlotSnapshot {
    /// Best-effort count of tokens currently occupying this slot's context.
    ///
    /// Tries, in priority order: `n_past`, then `cache_tokens`, then
    /// `next_token.n_decoded`. Returns `None` if the running llama-server
    /// version exposes none of them (shown as "unknown" by consumers,
    /// never treated as zero).
    #[must_use]
    pub fn tokens_in_use(&self) -> Option<u64> {
        self.n_past
            .or(self.cache_tokens)
            .or_else(|| self.next_token.as_ref().and_then(|nt| nt.n_decoded))
    }

    /// Remaining context budget for this slot (`n_ctx - tokens_in_use`).
    ///
    /// `None` if either the slot's context size or its token usage is
    /// unknown on this llama-server version.
    #[must_use]
    pub fn context_remaining(&self) -> Option<u64> {
        let n_ctx = self.n_ctx?;
        let used = self.tokens_in_use()?;
        Some(n_ctx.saturating_sub(used))
    }
}

// =============================================================================
// SlotsPollResult
// =============================================================================

/// Outcome of one `GET /slots` poll attempt. Always a displayable,
/// non-fatal state — this type is constructed by [`fetch_slots`], which
/// never panics regardless of what llama-server sends back.
///
/// Deliberately *not* `#[serde(tag = "...")]` (internally tagged): serde
/// cannot inject a tag key into the `Available` variant's payload because
/// it serializes as a JSON array (`Vec<SlotSnapshot>`), not an object —
/// internally-tagged newtype variants require map-shaped content, so that
/// representation fails at runtime for this enum. Callers that need a
/// stable JSON contract (e.g. [`crate::dashboard::DashboardSnapshot`])
/// flatten this enum into plain fields instead of serializing it directly.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SlotsPollResult {
    /// The endpoint responded with a well-formed slots array.
    Available(Vec<SlotSnapshot>),
    /// llama-server was started with `--no-slots`; confirmed via the
    /// documented `501` response. The poller should stop retrying for the
    /// remainder of this run (see [`crate::slots_poller`]).
    Disabled,
    /// The endpoint could not be reached, timed out, returned an
    /// unexpected status, or returned a body that failed to parse. The
    /// human-readable reason is kept for logging/display; callers should
    /// never `unwrap` on the underlying cause.
    Unreachable(String),
}

/// Parse a `GET /slots` response into a [`SlotsPollResult`], given its
/// status code and raw body. Pure and synchronous so it can be unit-tested
/// directly against fixtures without a live server.
fn parse_slots_response(status: StatusCode, body: &str) -> SlotsPollResult {
    if status == StatusCode::NOT_IMPLEMENTED {
        return SlotsPollResult::Disabled;
    }
    if !status.is_success() {
        return SlotsPollResult::Unreachable(format!("unexpected HTTP status {status}"));
    }
    match serde_json::from_str::<Vec<SlotSnapshot>>(body) {
        Ok(slots) => SlotsPollResult::Available(slots),
        Err(e) => SlotsPollResult::Unreachable(format!("failed to parse /slots response: {e}")),
    }
}

/// Fetch and parse `GET {base_url}/slots`.
///
/// Never panics: connection errors, timeouts, unexpected statuses, and
/// malformed bodies all resolve to a displayable [`SlotsPollResult`]
/// variant rather than an `Err`/panic.
pub async fn fetch_slots(client: &Client, base_url: &str) -> SlotsPollResult {
    let url = format!("{base_url}/slots");
    let response = match client.get(&url).timeout(SLOTS_REQUEST_TIMEOUT).send().await {
        Ok(r) => r,
        Err(e) => return SlotsPollResult::Unreachable(e.to_string()),
    };
    let status = response.status();
    let body = match response.text().await {
        Ok(b) => b,
        Err(e) => {
            return SlotsPollResult::Unreachable(format!(
                "failed to read /slots response body: {e}"
            ));
        }
    };
    parse_slots_response(status, &body)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim (trimmed) example from the current upstream llama.cpp docs
    /// (`tools/server/README.md`, "GET /slots" section, "Example with 2
    /// slots"). Notably has **no** `n_past`/`cache_tokens` field anywhere —
    /// this is the schema that motivated the `next_token.n_decoded`
    /// fallback.
    const CURRENT_SCHEMA_FIXTURE: &str = r#"[
        {
            "id": 0,
            "id_task": 135,
            "n_ctx": 65536,
            "speculative": false,
            "is_processing": true,
            "params": { "n_predict": -1, "seed": 4294967295 },
            "next_token": {
                "has_next_token": true,
                "has_new_line": false,
                "n_remain": -1,
                "n_decoded": 0
            }
        },
        {
            "id": 1,
            "id_task": 0,
            "n_ctx": 65536,
            "speculative": false,
            "is_processing": true,
            "params": { "n_predict": -1, "seed": 4294967295 },
            "next_token": {
                "has_next_token": true,
                "has_new_line": true,
                "n_remain": -1,
                "n_decoded": 136
            }
        }
    ]"#;

    /// A synthetic older-style fixture using the legacy flat `n_past` field
    /// and no `next_token` object at all, to confirm both schema shapes
    /// parse through the same struct.
    const LEGACY_SCHEMA_FIXTURE: &str = r#"[
        {
            "id": 0,
            "id_task": -1,
            "n_ctx": 4096,
            "is_processing": false,
            "n_past": 512
        }
    ]"#;

    const DISABLED_FIXTURE_BODY: &str = r#"{
        "error": {
            "code": 501,
            "message": "This server does not support slots endpoint.",
            "type": "not_supported_error"
        }
    }"#;

    #[test]
    fn parses_current_upstream_schema() {
        let result = parse_slots_response(StatusCode::OK, CURRENT_SCHEMA_FIXTURE);
        let SlotsPollResult::Available(slots) = result else {
            panic!("expected Available, got {result:?}");
        };
        assert_eq!(slots.len(), 2);

        assert_eq!(slots[0].id, 0);
        assert_eq!(slots[0].n_ctx, Some(65536));
        assert!(slots[0].is_processing);
        // No n_past/cache_tokens in this schema; falls back to next_token.n_decoded.
        assert_eq!(slots[0].tokens_in_use(), Some(0));
        assert_eq!(slots[0].context_remaining(), Some(65536));

        assert_eq!(slots[1].tokens_in_use(), Some(136));
        assert_eq!(slots[1].context_remaining(), Some(65536 - 136));
    }

    #[test]
    fn parses_legacy_schema_with_n_past() {
        let result = parse_slots_response(StatusCode::OK, LEGACY_SCHEMA_FIXTURE);
        let SlotsPollResult::Available(slots) = result else {
            panic!("expected Available, got {result:?}");
        };
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].tokens_in_use(), Some(512));
        assert_eq!(slots[0].context_remaining(), Some(4096 - 512));
    }

    #[test]
    fn n_past_takes_priority_over_next_token_when_both_present() {
        let body = r#"[{
            "id": 0,
            "n_ctx": 1000,
            "is_processing": true,
            "n_past": 100,
            "next_token": { "n_decoded": 999 }
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(slots[0].tokens_in_use(), Some(100));
    }

    #[test]
    fn tokens_in_use_is_none_when_no_candidate_field_present() {
        let body = r#"[{ "id": 0, "n_ctx": 1000, "is_processing": false }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(slots[0].tokens_in_use(), None);
        assert_eq!(slots[0].context_remaining(), None);
    }

    #[test]
    fn empty_slots_array_is_available_with_no_slots() {
        let result = parse_slots_response(StatusCode::OK, "[]");
        assert_eq!(result, SlotsPollResult::Available(vec![]));
    }

    #[test]
    fn http_501_is_disabled() {
        let result = parse_slots_response(StatusCode::NOT_IMPLEMENTED, DISABLED_FIXTURE_BODY);
        assert_eq!(result, SlotsPollResult::Disabled);
    }

    #[test]
    fn unexpected_status_is_unreachable_not_panic() {
        let result = parse_slots_response(StatusCode::INTERNAL_SERVER_ERROR, "oops");
        assert!(matches!(result, SlotsPollResult::Unreachable(_)));
    }

    #[test]
    fn malformed_body_is_unreachable_not_panic() {
        let result = parse_slots_response(StatusCode::OK, "{ this is not valid json [[[");
        assert!(matches!(result, SlotsPollResult::Unreachable(_)));
    }

    #[test]
    fn body_that_is_valid_json_but_wrong_shape_is_unreachable_not_panic() {
        // A JSON object (not an array) — e.g. a future schema change or a
        // proxy/error page returning `{}`.
        let result = parse_slots_response(StatusCode::OK, r#"{"unexpected": true}"#);
        assert!(matches!(result, SlotsPollResult::Unreachable(_)));
    }
}
