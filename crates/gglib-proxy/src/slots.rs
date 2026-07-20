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
//! reports prompt usage via `n_prompt_tokens`/`n_prompt_tokens_processed`
//! and nests generation progress under `next_token`, dropping `n_past`
//! entirely; builds with Multi-Token Prediction, aka "draft-mtp", send
//! `next_token` as an **array** of objects instead of a single object).
//! Rather than accepting an untyped [`serde_json::Value`] and probing it
//! ad hoc at every call site, [`SlotSnapshot`] declares only the handful of
//! fields the dashboard actually needs, with `Option<T>` (plus
//! `#[serde(default)]`) on every field whose presence varies by version,
//! plus [`tolerant_u64`] on every numeric field whose *type* has been known
//! to shift (so a future schema change degrades that one field to `None`
//! instead of failing the entire response). Fields we don't care about
//! (`params`, `speculative`, etc.) are simply never named, so serde
//! silently drops them — no `deny_unknown_fields`, no brittle JSON-pointer
//! probing, no risk of a partially-unknown schema causing the whole
//! response to fail to parse.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashSet;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::time as tokio_time;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Tolerant `u64` deserializer: decodes a JSON number as usual, but treats
/// any other JSON type (object, array, string, bool, `null`) as simply
/// absent (`None`) rather than a hard parse error.
///
/// llama.cpp's `/slots` schema has changed the *type* of individual fields
/// across versions (not just their presence) — e.g. a future build could
/// promote `n_ctx` or `cache_tokens` to a nested object the way `next_token`
/// already has been. Without this, a single unexpected field type fails
/// `serde_json::from_str::<Vec<SlotSnapshot>>` for the *entire* `/slots`
/// response, dropping data for every slot rather than just the one field
/// that changed shape.
fn tolerant_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|v| v.as_u64()))
}

/// Per-request timeout for `GET /slots` polls.
///
/// llama-server serves `/slots` from the same single HTTP thread that
/// processes prompts, so a poll can legitimately block for as long as the
/// server is busy with prompt evaluation — this is normal backpressure, not
/// a hang. With a large context window (e.g. 131k tokens), a full prompt
/// fill can take on the order of `prompt eval time` scaled up from typical
/// measurements like `30.5s / 4181 tokens`, i.e. upwards of 15+ minutes.
///
/// This timeout is independent of the main proxy client's (intentionally
/// unbounded) chat-completion timeout. It is deliberately generous rather
/// than short: `fetch_slots` is only ever called from `spawn_slots_poller`'s
/// isolated `tokio::spawn` task, so a slow poll can only delay that task's
/// own next tick — it never blocks Axum request handling, chat-completion
/// forwarding, or the dashboard's SSE stream. A poll that genuinely never
/// returns (dead server, not just a busy one) is still eventually caught,
/// just after a much more forgiving wait.
const SLOTS_REQUEST_TIMEOUT: Duration = Duration::from_secs(900);

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
    #[serde(default, deserialize_with = "tolerant_u64")]
    pub n_ctx: Option<u64>,
    /// Whether this slot is actively processing a request right now.
    #[serde(default)]
    pub is_processing: bool,
    /// Legacy field (older llama.cpp versions): tokens already resident in
    /// this slot's KV cache. Superseded by `n_prompt_tokens`/`next_token` in
    /// current upstream versions, where it is simply absent — kept only as
    /// a best-effort fallback, see [`Self::tokens_in_use`].
    #[serde(default, deserialize_with = "tolerant_u64")]
    n_past: Option<u64>,
    /// Alternate legacy field name seen in some intermediate llama.cpp
    /// builds; same role as `n_past`.
    #[serde(default, deserialize_with = "tolerant_u64")]
    cache_tokens: Option<u64>,
    /// Current-schema field: total token count of the prompt currently
    /// loaded into this slot (the "prompt half" of context usage, as
    /// opposed to `next_token.n_decoded`'s generated-token count).
    #[serde(default, deserialize_with = "tolerant_u64")]
    n_prompt_tokens: Option<u64>,
    /// Current-schema field: how many of `n_prompt_tokens` have actually
    /// been processed (prefilled) so far, i.e. run through the model this
    /// round — this is a *delta*, excluding anything reused from cache.
    /// Preferred over `n_prompt_tokens` itself when present, since it
    /// tracks real progress during an in-flight prefill rather than the
    /// eventual total; must be combined with `n_prompt_tokens_cache` to
    /// recover the true total prompt usage, see [`Self::tokens_in_use`].
    #[serde(default, deserialize_with = "tolerant_u64")]
    n_prompt_tokens_processed: Option<u64>,
    /// Current-schema field: number of prompt tokens reused from KV cache
    /// (prefix-match reuse across requests to the same slot), i.e. *not*
    /// re-run through the model this round. Mirrors the `cache_n` field in
    /// the `/v1/chat/completions` response's `timings` object, where
    /// llama.cpp documents the invariant `prompt_n + cache_n + predicted_n`
    /// as the total tokens in context — only meaningful alongside
    /// `n_prompt_tokens_processed` (the `prompt_n` analogue); adding it to
    /// the grand-total `n_prompt_tokens` fallback would double-count.
    #[serde(default, deserialize_with = "tolerant_u64")]
    n_prompt_tokens_cache: Option<u64>,
    /// Current-schema nested object carrying generation progress. We only
    /// need `n_decoded` out of it — the count of tokens generated so far,
    /// which is additive with `n_prompt_tokens(_processed)` to get total
    /// context usage, see [`Self::tokens_in_use`].
    ///
    /// On llama.cpp builds with Multi-Token Prediction ("draft-mtp")
    /// enabled, upstream sends this as a JSON **array** of objects (one per
    /// predicted token) instead of a single object — see [`NextTokenField`].
    #[serde(default)]
    next_token: Option<NextTokenField>,
}

/// The subset of the current schema's `next_token` object we care about.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct NextTokenInfo {
    /// Number of tokens decoded (generated) so far for the active task.
    #[serde(default, deserialize_with = "tolerant_u64")]
    n_decoded: Option<u64>,
}

/// `next_token` can be either a single object (regular builds) or an array
/// of objects (Multi-Token Prediction / "draft-mtp" builds, one entry per
/// predicted token). `#[serde(untagged)]` tries each variant in declared
/// order until one succeeds.
///
/// `Many` **must** be declared before `Single`: since every field on
/// [`NextTokenInfo`] is `Option` + `#[serde(default)]`, serde's derived
/// struct deserializer also accepts a JSON *array* as positional field
/// values (not just a map) — so a single-element array like
/// `[{"n_decoded": 89}]` would otherwise wrongly succeed as `Single` first
/// (assigning the whole inner object to the `n_decoded` field, which
/// `tolerant_u64` then silently downgrades to `None` instead of erroring).
/// Trying `Many` first avoids this false-positive match.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
enum NextTokenField {
    Many(Vec<NextTokenInfo>),
    Single(NextTokenInfo),
}

impl NextTokenField {
    /// The representative entry to read progress from: the object itself
    /// for the single-object shape, or element 0 for the MTP array shape
    /// (the accepted/main decode stream — later elements are speculative
    /// draft predictions, not relevant to a "tokens in use" signal).
    fn primary(&self) -> Option<&NextTokenInfo> {
        match self {
            Self::Single(info) => Some(info),
            Self::Many(items) => items.first(),
        }
    }
}

impl SlotSnapshot {
    /// Best-effort count of tokens currently occupying this slot's context.
    ///
    /// Current-schema builds report prompt usage and generation progress as
    /// separate counters that must be **added together** to get the true
    /// total (a 20k-token prompt with 89 tokens generated so far is ~20k
    /// tokens in use, not 89) — mirroring the invariant llama.cpp documents
    /// for `/v1/chat/completions`' `timings` object: total context =
    /// `prompt_n + cache_n + predicted_n`.
    ///
    /// When `n_prompt_tokens_processed` is present, it is preferred over
    /// `n_prompt_tokens` (it tracks real progress mid-prefill rather than
    /// the eventual total) and is combined with `n_prompt_tokens_cache`
    /// (tokens reused from KV cache this round, not re-run through the
    /// model) — without this, a cache hit on a follow-up prompt makes
    /// context usage appear to collapse to just the small newly-processed
    /// delta. `next_token.n_decoded` is added on top of either (defaulting
    /// to 0 if generation hasn't started yet).
    ///
    /// When only the grand-total `n_prompt_tokens` is present (no
    /// `_processed` reported), it is used as-is — it already represents
    /// the full prompt including any cached prefix, so `n_prompt_tokens_cache`
    /// is **not** added on top of it (that would double-count).
    ///
    /// Only when **neither** prompt-side field is present (older
    /// llama-server versions, which never report them alongside
    /// `next_token`) does this fall back to the legacy, non-additive chain:
    /// `n_past`, then `cache_tokens`, then `next_token.n_decoded` alone.
    ///
    /// Returns `None` if the running llama-server version exposes none of
    /// these fields (shown as "unknown" by consumers, never treated as
    /// zero).
    #[must_use]
    pub fn tokens_in_use(&self) -> Option<u64> {
        let n_decoded = self
            .next_token
            .as_ref()
            .and_then(NextTokenField::primary)
            .and_then(|nt| nt.n_decoded);

        let prompt_component = if let Some(processed) = self.n_prompt_tokens_processed {
            Some(processed + self.n_prompt_tokens_cache.unwrap_or(0))
        } else {
            self.n_prompt_tokens
        };

        if let Some(prompt_tokens) = prompt_component {
            return Some(prompt_tokens + n_decoded.unwrap_or(0));
        }

        self.n_past.or(self.cache_tokens).or(n_decoded)
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

    /// Exact (trimmed) payload shape reported from a live llama.cpp
    /// `8c146a8` build with Multi-Token Prediction ("draft-mtp") enabled:
    /// `next_token` is a single-element array, not a bare object.
    ///
    /// `tokens_in_use()` must be `n_prompt_tokens_processed + n_decoded`
    /// (20906 + 89 = 20995), NOT just `n_decoded` (89) — a prior version of
    /// this test asserted `Some(89)`, which enshrined the real bug reported
    /// against this exact payload: a 20k+-token prompt showing as ~0% used
    /// because only the generated-token count was read.
    #[test]
    fn parses_mtp_array_next_token_schema() {
        let body = r#"[{
            "id": 3,
            "n_ctx": 131072,
            "is_processing": true,
            "n_prompt_tokens": 20994,
            "n_prompt_tokens_processed": 20906,
            "n_prompt_tokens_cache": 0,
            "next_token": [
                { "n_remain": 8103, "n_decoded": 89 }
            ]
        }]"#;
        let result = parse_slots_response(StatusCode::OK, body);
        let SlotsPollResult::Available(slots) = result else {
            panic!("expected Available, got {result:?}");
        };
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].n_ctx, Some(131072));
        assert_eq!(slots[0].tokens_in_use(), Some(20906 + 89));
        assert_eq!(slots[0].context_remaining(), Some(131072 - (20906 + 89)));
    }

    #[test]
    fn n_prompt_tokens_processed_takes_priority_over_n_prompt_tokens() {
        let body = r#"[{
            "id": 0,
            "n_ctx": 1000,
            "is_processing": true,
            "n_prompt_tokens": 500,
            "n_prompt_tokens_processed": 300,
            "next_token": [{ "n_decoded": 10 }]
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(
            slots[0].tokens_in_use(),
            Some(310),
            "should prefer n_prompt_tokens_processed (300) over n_prompt_tokens (500), plus n_decoded"
        );
    }

    #[test]
    fn falls_back_to_n_prompt_tokens_when_processed_absent() {
        let body = r#"[{
            "id": 0,
            "n_ctx": 1000,
            "is_processing": true,
            "n_prompt_tokens": 500,
            "next_token": [{ "n_decoded": 10 }]
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(slots[0].tokens_in_use(), Some(510));
    }

    /// Real-world KV-cache-reuse scenario: a follow-up prompt where
    /// llama-server found a large cached prefix match (`f_keep` close to 1)
    /// and only had to newly process a small delta. `n_prompt_tokens_cache`
    /// must be added to `n_prompt_tokens_processed`, or context usage would
    /// falsely collapse to just the tiny newly-processed delta on every
    /// cache-hit follow-up turn.
    #[test]
    fn cache_reuse_adds_n_prompt_tokens_cache_to_processed_delta() {
        let body = r#"[{
            "id": 0,
            "n_ctx": 131072,
            "is_processing": true,
            "n_prompt_tokens": 7981,
            "n_prompt_tokens_processed": 1245,
            "n_prompt_tokens_cache": 6736,
            "next_token": [{ "n_decoded": 12 }]
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(
            slots[0].tokens_in_use(),
            Some(1245 + 6736 + 12),
            "must be processed + cache + decoded, not just processed + decoded (1257)"
        );
    }

    /// The grand-total `n_prompt_tokens` fallback (used only when
    /// `n_prompt_tokens_processed` is absent) already includes any cached
    /// prefix, so `n_prompt_tokens_cache` must NOT be added on top of it —
    /// doing so would double-count the cached tokens.
    #[test]
    fn n_prompt_tokens_cache_is_not_double_counted_against_the_grand_total() {
        let body = r#"[{
            "id": 0,
            "n_ctx": 1000,
            "is_processing": true,
            "n_prompt_tokens": 500,
            "n_prompt_tokens_cache": 400,
            "next_token": [{ "n_decoded": 10 }]
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(
            slots[0].tokens_in_use(),
            Some(510),
            "n_prompt_tokens (500) already includes the cached prefix; adding cache (400) again would double-count"
        );
    }

    #[test]
    fn prompt_tokens_present_but_generation_not_started_yet() {
        let body = r#"[{
            "id": 0,
            "n_ctx": 1000,
            "is_processing": true,
            "n_prompt_tokens": 500,
            "n_prompt_tokens_processed": 250
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(
            slots[0].tokens_in_use(),
            Some(250),
            "no next_token yet: n_decoded contribution should default to 0"
        );
    }

    #[test]
    fn mtp_array_with_multiple_predictions_uses_first_element() {
        let body = r#"[{
            "id": 0,
            "n_ctx": 1000,
            "is_processing": true,
            "next_token": [
                { "n_remain": 10, "n_decoded": 42 },
                { "n_remain": 9, "n_decoded": 999 }
            ]
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available");
        };
        assert_eq!(
            slots[0].tokens_in_use(),
            Some(42),
            "should use element 0 (accepted stream), not later speculative entries"
        );
    }

    #[test]
    fn tolerant_u64_field_downgrades_to_none_not_parse_error() {
        // n_ctx sent as a nested object — a hypothetical future schema
        // change. Must not fail the whole `/slots` parse.
        let body = r#"[{
            "id": 0,
            "n_ctx": { "unexpected": "shape" },
            "is_processing": false
        }]"#;
        let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, body) else {
            panic!("expected Available, not Unreachable, when only n_ctx's type changes");
        };
        assert_eq!(slots[0].n_ctx, None);
    }

    #[test]
    fn tolerant_u64_downgrades_n_past_and_cache_tokens_too() {
        for field in ["n_past", "cache_tokens"] {
            let body = format!(
                r#"[{{ "id": 0, "n_ctx": 1000, "is_processing": false, "{field}": [1, 2, 3] }}]"#
            );
            let SlotsPollResult::Available(slots) = parse_slots_response(StatusCode::OK, &body)
            else {
                panic!("expected Available when only '{field}' has an unexpected type");
            };
            assert_eq!(
                slots[0].tokens_in_use(),
                None,
                "'{field}' with an unexpected type should degrade to None, not panic/error"
            );
        }
    }
}

// =============================================================================
// Slot I/O — save / restore / clear + background LRU eviction
// =============================================================================

/// Result of a slot I/O operation (save/restore).
///
/// `NotFound` means no cached slot exists for this session (not an error —
/// the request proceeds cold). `Transient` failures may be retried.
/// `Permanent` failures are terminal (e.g., invalid session ID).
#[derive(Debug, PartialEq)]
pub enum SlotIoResult {
    Ok,
    NotFound,
    Transient(String),
    Permanent(String),
}

/// Sanitize session ID for use as a filename. Alphanumeric + hyphens/underscores, max 64 chars.
///
/// Lowercased on return: the ID becomes a `.bin` filename, and the default
/// filesystem on this project's primary dev/deploy platform (macOS/APFS) is
/// case-insensitive-but-preserving, so `"Planner"` and `"planner"` would
/// otherwise silently collide onto one file and cross-contaminate two
/// distinct sessions' KV caches.
pub fn sanitize_session_id(id: &str) -> Result<String, String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("invalid session ID: {:?}", id));
    }
    Ok(id.to_lowercase())
}

/// Return the full path to a slot `.bin` file for a given model and session.
/// Uses the shared `slot_bin_path()` from gglib-core so both runtime (purge)
/// and proxy (save/restore) agree on `{slot_dir}/{model_id}__{session}.bin`.
pub fn slot_bin_path(slot_dir: &Path, model_id: u32, session_id: &str) -> PathBuf {
    gglib_core::paths::slot_bin_path(slot_dir, model_id, session_id)
}

/// Pure classification of a slot save/restore result from its status + body.
///
/// - 2xx → `Ok`
/// - 404 → `NotFound` (no cached file — proceed cold, not an error)
/// - other 4xx → `Permanent`: a client error will never succeed on retry
///   (e.g. `"Invalid filename"`, or a corrupt/incompatible `"invalid slot
///   save file"`). Retrying just burns the slot semaphore for ~200ms and
///   re-logs; these must be terminal.
/// - 5xx → `Transient`: a server-side/transient condition worth retrying.
///
/// The response `body` is included in the message either way — llama-server
/// puts the actual reason there, otherwise invisible behind a bare status.
fn classify_slot_status(status: StatusCode, body: &str) -> SlotIoResult {
    if status.is_success() {
        return SlotIoResult::Ok;
    }
    if status.as_u16() == 404 {
        return SlotIoResult::NotFound;
    }
    let msg = format!("HTTP {status}: {}", body.trim());
    if status.is_client_error() {
        SlotIoResult::Permanent(msg)
    } else {
        SlotIoResult::Transient(msg)
    }
}

/// Read a slot save/restore HTTP response and classify it. Shared by
/// [`save_slot`] and [`restore_slot`], which differ only in the endpoint hit.
async fn classify_slot_response(resp: reqwest::Response) -> SlotIoResult {
    let status = resp.status();
    // Only pay for the body read on a non-success status.
    if status.is_success() {
        return SlotIoResult::Ok;
    }
    let body = resp.text().await.unwrap_or_default();
    classify_slot_status(status, &body)
}

/// Generous timeout for a slot save. Live slot files run 2-6.4 GB, so the
/// previous 3s budget was never enough to complete a write; a save that hit
/// it did not stop llama-server's write, it just stopped *waiting* for it,
/// so a retry could race a still-writing prior attempt onto the same file.
/// That race is now impossible: [`save_slot`] asks the server to write a
/// per-attempt temp name and only renames it onto the real `.bin` name after
/// a confirmed-complete write, so a generous timeout here costs nothing but
/// time.
const SAVE_TIMEOUT: Duration = Duration::from_secs(120);

/// Generous timeout for a slot restore — reading a multi-GB file back from
/// disk (cold cache, slow storage) can take a while.
const RESTORE_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-attempt nonce for temp save filenames, so a timed-out-but-still-writing
/// previous attempt can never collide with a retry's temp file.
static SAVE_NONCE: AtomicU64 = AtomicU64::new(0);

/// Rename a completed temp save onto its final `.bin` name and return its
/// size in bytes. Split out from [`save_slot`] so the rename step is unit
/// testable without an HTTP server.
async fn finalize_slot_save(tmp_path: &Path, dest_path: &Path) -> std::io::Result<u64> {
    tokio::fs::rename(tmp_path, dest_path).await?;
    Ok(tokio::fs::metadata(dest_path).await?.len())
}

/// Trigger a KV cache save for the current slot via llama-server's `/slots/0?action=save`.
///
/// llama-server is asked to write to a per-attempt temp filename (relative to
/// its `--slot-save-path`, flat and separator-free — it rejects a `/` with
/// HTTP 400 "Invalid filename"); on success this renames the temp file onto
/// the real `{model_id}__{session_id}.bin` name so restore/eviction, which
/// only ever look at `*.bin`, never see a partially-written file. Returns
/// `SlotIoResult::Ok` on success, `NotFound` if the server returns 404, or
/// `Transient` for timeout/network/rename errors. On any non-`Ok` outcome the
/// temp file is left behind (the server may still be writing it); the
/// eviction sweep reaps orphaned temp files older than its staleness window.
pub async fn save_slot(
    client: &Client,
    base_url: &str,
    slot_dir: &Path,
    model_id: u32,
    session_id: &str,
) -> SlotIoResult {
    // Ensure the (flat) slot directory exists before saving.
    if let Err(e) = tokio::fs::create_dir_all(slot_dir).await {
        warn!(
            "failed to create slot directory {}: {}",
            slot_dir.display(),
            e
        );
        return SlotIoResult::Transient(format!("failed to create slot directory: {e}"));
    }
    let nonce = SAVE_NONCE.fetch_add(1, Ordering::Relaxed);
    let tmp_name = gglib_core::paths::slot_tmp_file_name(model_id, session_id, nonce);
    let payload = serde_json::json!({"filename": &tmp_name});
    let started = Instant::now();
    let result = tokio_time::timeout(
        SAVE_TIMEOUT,
        client
            .post(format!("{base_url}/slots/0?action=save"))
            .json(&payload)
            .send(),
    )
    .await;

    let classified = match result {
        Ok(Ok(resp)) => classify_slot_response(resp).await,
        Ok(Err(e)) => SlotIoResult::Transient(e.to_string()),
        Err(_) => SlotIoResult::Transient(format!("timeout after {}s", SAVE_TIMEOUT.as_secs())),
    };

    if !matches!(classified, SlotIoResult::Ok) {
        return classified;
    }

    let tmp_path = slot_dir.join(&tmp_name);
    let dest_path = slot_bin_path(slot_dir, model_id, session_id);
    match finalize_slot_save(&tmp_path, &dest_path).await {
        Ok(bytes) => {
            info!(
                "saved slot for {session_id}: {:.2} GiB in {:.1}s",
                bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                started.elapsed().as_secs_f64()
            );
            SlotIoResult::Ok
        }
        Err(e) => SlotIoResult::Transient(format!("failed to finalize slot save: {e}")),
    }
}

/// Check whether a session's `.bin` slot file predates `server_start_secs`.
///
/// A slot file older than the current llama-server process's start time was
/// written by a *prior* process instance and is not safe to restore into
/// this one (spawn-time purge is the primary defense — see
/// `purge_stale_slot_bin_files` in `gglib-runtime` — this is a second,
/// independent layer in case that purge is ever bypassed, e.g. a mismatched
/// `--slot-save-path`).
///
/// Fail-open on every uncertain case — a missing file, an unreadable mtime,
/// or `server_start_secs == 0` (guard not yet initialized) all return
/// `false` ("not stale"), so the normal restore attempt proceeds and either
/// succeeds or hits its own 404. This function only ever prevents a restore
/// it can positively prove is stale; it never blocks one it can't evaluate.
pub(crate) async fn slot_file_is_stale(
    slot_dir: &Path,
    model_id: u32,
    session_id: &str,
    server_start_secs: u64,
) -> bool {
    if server_start_secs == 0 {
        return false;
    }
    let path = slot_bin_path(slot_dir, model_id, session_id);
    let Ok(metadata) = tokio::fs::metadata(&path).await else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let Ok(mtime_secs) = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
    else {
        return false;
    };
    mtime_secs < server_start_secs
}

/// Trigger a KV cache restore for the current slot via llama-server's `/slots/0?action=restore`.
///
/// Returns `SlotIoResult::Ok` on success, `NotFound` if no cached file exists (404),
/// or `Transient` for timeout/network errors. [`RESTORE_TIMEOUT`] is generous
/// because restore may need to read a multi-GB file from disk.
pub async fn restore_slot(
    client: &Client,
    base_url: &str,
    _slot_dir: &Path,
    model_id: u32,
    session_id: &str,
) -> SlotIoResult {
    // Filename is relative to --slot-save-path; flat, separator-free.
    let filename = gglib_core::paths::slot_file_name(model_id, session_id);
    let payload = serde_json::json!({"filename": &filename});
    match tokio_time::timeout(
        RESTORE_TIMEOUT,
        client
            .post(format!("{base_url}/slots/0?action=restore"))
            .json(&payload)
            .send(),
    )
    .await
    {
        Ok(Ok(resp)) => classify_slot_response(resp).await,
        Ok(Err(e)) => SlotIoResult::Transient(e.to_string()),
        Err(_) => {
            warn!("restore timed out for {session_id} — proceeding cold");
            SlotIoResult::Transient(format!("timeout after {}s", RESTORE_TIMEOUT.as_secs()))
        }
    }
}

/// Clear per-slot cache files from disk. Uses tokio::fs for async safety.
///
/// Slot files are flat as `{slot_dir}/{model_id}__{session}.bin`.
///
/// `session_id: Some(id)` removes every `.bin` file whose encoded session is
/// `id`, regardless of which model prefixed it (a session could in principle
/// have been cached under more than one model over its lifetime). `None`
/// clears everything.
pub async fn clear_slot_files(slot_dir: &Path, session_id: Option<&str>) -> std::io::Result<()> {
    let candidates = iter_all_slot_files(slot_dir).await;
    for path in candidates {
        let matches = match session_id {
            Some(id) => {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(gglib_core::paths::slot_session_from_stem)
                    == Some(id)
            }
            None => true,
        };
        if matches
            && let Err(e) = tokio::fs::remove_file(&path).await
            && e.kind() != std::io::ErrorKind::PermissionDenied
            && e.kind() != std::io::ErrorKind::NotFound
        {
            warn!("failed to remove slot file {}: {}", path.display(), e);
        }
    }
    Ok(())
}

/// Retry budget for a transient slot I/O failure — shared with
/// `cache_lifecycle::restore_with_retry`'s retry loop (`slots.rs` sits below
/// `cache_lifecycle.rs` in the module layering, so the constants live here
/// and `cache_lifecycle` imports them).
///
/// A stale pooled HTTP connection (llama-server closes idle keep-alive
/// connections; a 10+ minute generation easily outlives one) produces an
/// instant `Transient` "error sending request" failure on the very next
/// call — indistinguishable at the transport level from any other
/// transient error, and exactly the case `restore_slot` already retries.
/// Without this, a save silently drops with a single WARN, leaving the
/// on-disk `.bin` permanently stale relative to what's actually in the
/// slot's live KV cache — a mismatch that a later restore would then load
/// as if it were current.
pub(crate) const MAX_RETRIES: u32 = 2;
pub(crate) const RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// Shared save function — called by both streaming and non-streaming paths.
pub async fn attempt_save(
    client: &Client,
    base_url: &str,
    slot_dir: &Path,
    model_id: u32,
    session_id: &str,
    clear_all_pending: &AtomicBool,
    per_session_cleared: &DashSet<String>,
) {
    if clear_all_pending.load(Ordering::SeqCst) {
        debug!("skipping save for {session_id} — clear_all_pending");
        return;
    }
    if per_session_cleared.contains(session_id) {
        debug!("skipping save for {session_id} — cleared mid-generation");
        return;
    }

    let mut result = save_slot(client, base_url, slot_dir, model_id, session_id).await;
    if matches!(result, SlotIoResult::Transient(_)) {
        for attempt in 1..=MAX_RETRIES {
            debug!(
                "retry save for {session_id} (attempt {}/{})",
                attempt, MAX_RETRIES
            );
            tokio_time::sleep(RETRY_BACKOFF).await;
            result = save_slot(client, base_url, slot_dir, model_id, session_id).await;
            if !matches!(result, SlotIoResult::Transient(_)) {
                break;
            }
        }
    }

    match result {
        SlotIoResult::Ok => {}
        SlotIoResult::NotFound => warn!("save failed for {session_id}: 404 Not Found"),
        SlotIoResult::Transient(e) => {
            warn!("save failed for {session_id} after retries: {e}")
        }
        SlotIoResult::Permanent(e) => {
            warn!("save failed for {session_id} (permanent): {e}")
        }
    }
}

/// Iterate all `.bin` slot files directly under `slot_dir`.
///
/// Slot files are stored flat as `{slot_dir}/{model_id}__{session}.bin`.
/// Returns a `Vec<PathBuf>` sorted by mtime (oldest first). Used by LRU
/// eviction and cache-clear operations that need the complete set of cached
/// slots regardless of model.
pub async fn iter_all_slot_files(slot_dir: &Path) -> Vec<PathBuf> {
    let mut entries = match tokio::fs::read_dir(slot_dir).await {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut slots: Vec<PathBuf> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("bin") {
            slots.push(path);
        }
    }
    // Sort by mtime oldest-first so LRU eviction removes the least-recently-used first
    slots.sort_by_key(|p| {
        p.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    slots
}

/// Default cap on cached session slot files before LRU eviction kicks in.
///
/// No CLI knob exists for this yet — a fixed cap is strictly better than the
/// previous behavior of never evicting at all. Revisit if usage shows the
/// default is wrong for a given `--slot-dir` size budget.
pub const DEFAULT_MAX_CACHED_SESSIONS: usize = 100;

/// Background LRU eviction task — spawned at server startup, runs every 60s.
///
/// Exits promptly on `cancel` so it never outlives the server (same shutdown
/// contract as `spawn_slots_poller`/`spawn_dashboard_publisher`).
pub fn spawn_lru_eviction_task(
    slot_dir: PathBuf,
    max_slots: usize,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(60);
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                () = tokio::time::sleep(interval) => {
                    if let Err(e) = evict_stale_slots(&slot_dir, max_slots).await {
                        warn!("LRU eviction failed: {}", e);
                    }
                }
            }
        }
    })
}

/// Evict least-recently-used slot files from `slot_dir` when count exceeds `max_slots`.
///
/// Sorts `.bin` files by mtime (oldest first), removes the excess. Errors on
/// individual file removal are silently skipped for NotFound/PermissionDenied.
///
/// Walks the namespaced `{slot_dir}/{model_id}/*.bin` layout via
/// [`iter_all_slot_files`] — the cap is deliberately global across every
/// model's subdirectory (a single size budget for the whole cache directory,
/// not per-model).
pub async fn evict_stale_slots(slot_dir: &Path, max_slots: usize) -> std::io::Result<()> {
    let mut slots: Vec<(PathBuf, u64)> = Vec::new();
    for path in iter_all_slot_files(slot_dir).await {
        if let Ok(metadata) = tokio::fs::metadata(&path).await
            && let Ok(mtime) = metadata.modified()
        {
            slots.push((path, mtime.elapsed().unwrap_or_default().as_secs()));
        }
    }

    let excess_slots = sort_slots_for_eviction(slots, max_slots);
    for path in excess_slots {
        if let Err(e) = tokio::fs::remove_file(&path).await
            && e.kind() != std::io::ErrorKind::PermissionDenied
            && e.kind() != std::io::ErrorKind::NotFound
        {
            warn!("eviction failed for {}: {}", path.display(), e);
        }
    }
    Ok(())
}

/// Isolated sorting logic to allow pure unit testing without relying on filesystem mtimes.
fn sort_slots_for_eviction(mut slots: Vec<(PathBuf, u64)>, max_slots: usize) -> Vec<PathBuf> {
    slots.sort_by_key(|(_, age)| Reverse(*age)); // Oldest (largest age) first
    let excess = slots.len().saturating_sub(max_slots);
    slots.into_iter().take(excess).map(|(p, _)| p).collect()
}

// =============================================================================
// Slot I/O Tests
// =============================================================================

#[cfg(test)]
mod slot_io_tests {
    use super::*;

    #[test]
    fn test_sanitize_session_id() {
        assert_eq!(sanitize_session_id("planner").unwrap(), "planner");
        assert_eq!(sanitize_session_id("valid-id_123").unwrap(), "valid-id_123");
        assert!(sanitize_session_id("").is_err());
        assert!(sanitize_session_id("../invalid").is_err());
        assert!(sanitize_session_id("invalid/path").is_err());
        assert!(sanitize_session_id(&"a".repeat(65)).is_err()); // > 64 chars
    }

    /// Regression test: mixed-case IDs must not collide on case-insensitive
    /// filesystems (macOS/APFS default) once used as `.bin` filenames.
    #[test]
    fn test_sanitize_session_id_lowercases() {
        assert_eq!(sanitize_session_id("Planner").unwrap(), "planner");
        assert_eq!(
            sanitize_session_id("Planner").unwrap(),
            sanitize_session_id("planner").unwrap(),
            "differently-cased headers must resolve to the same session bucket"
        );
    }

    #[tokio::test]
    async fn finalize_slot_save_renames_tmp_to_final_and_returns_byte_count() {
        let dir = tempfile::tempdir().unwrap();
        let tmp_path = dir.path().join("1__planner.0.tmp");
        let dest_path = slot_bin_path(dir.path(), 1, "planner");
        let content = b"fake kv state, sixteen bytes!!!";
        std::fs::write(&tmp_path, content).unwrap();

        let bytes = finalize_slot_save(&tmp_path, &dest_path).await.unwrap();

        assert_eq!(bytes, content.len() as u64);
        assert!(!tmp_path.exists(), "tmp file should be gone after rename");
        assert!(dest_path.exists(), "final .bin should exist after rename");
    }

    #[tokio::test]
    async fn finalize_slot_save_visible_to_iter_all_slot_files() {
        let dir = tempfile::tempdir().unwrap();
        let tmp_path = dir.path().join("1__planner.0.tmp");
        let dest_path = slot_bin_path(dir.path(), 1, "planner");
        std::fs::write(&tmp_path, b"x").unwrap();

        finalize_slot_save(&tmp_path, &dest_path).await.unwrap();

        let files = iter_all_slot_files(dir.path()).await;
        assert_eq!(files, vec![dest_path]);
    }

    #[tokio::test]
    async fn finalize_slot_save_errors_when_tmp_missing() {
        let dir = tempfile::tempdir().unwrap();
        let tmp_path = dir.path().join("1__ghost.0.tmp");
        let dest_path = slot_bin_path(dir.path(), 1, "ghost");

        assert!(finalize_slot_save(&tmp_path, &dest_path).await.is_err());
    }

    #[test]
    fn test_lru_sort_logic() {
        let slots = vec![
            (PathBuf::from("A.bin"), 60),   // 60 seconds old
            (PathBuf::from("B.bin"), 3600), // 1 hour old (oldest)
            (PathBuf::from("C.bin"), 10),   // 10 seconds old (newest)
        ];

        let evicted = sort_slots_for_eviction(slots, 2);

        // With max_slots=2, the single oldest slot (B) should be evicted
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], PathBuf::from("B.bin"));
    }

    /// `evict_stale_slots` must see all flat `{model}__{session}.bin` files
    /// regardless of which model prefixed them.
    #[tokio::test]
    async fn evict_stale_slots_sees_files_across_models() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();

        // Three files total across two models, oldest first.
        std::fs::write(slot_bin_path(d, 1, "oldest"), b"x").unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        std::fs::write(slot_bin_path(d, 2, "middle"), b"x").unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        std::fs::write(slot_bin_path(d, 1, "newest"), b"x").unwrap();

        evict_stale_slots(d, 2).await.unwrap();

        // The single oldest file (1__oldest.bin) should have been evicted;
        // the other two, across both models, must survive.
        assert!(!slot_bin_path(d, 1, "oldest").exists());
        assert!(slot_bin_path(d, 2, "middle").exists());
        assert!(slot_bin_path(d, 1, "newest").exists());
    }

    /// Clearing a specific session must find its file regardless of which
    /// model prefixed it, and must not touch other sessions or other models.
    #[tokio::test]
    async fn clear_slot_files_finds_session_across_models() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();

        std::fs::write(slot_bin_path(d, 1, "planner"), b"x").unwrap();
        std::fs::write(slot_bin_path(d, 2, "coder"), b"x").unwrap();

        clear_slot_files(d, Some("planner")).await.unwrap();

        assert!(!slot_bin_path(d, 1, "planner").exists());
        assert!(
            slot_bin_path(d, 2, "coder").exists(),
            "other session untouched"
        );
    }

    #[test]
    fn classify_2xx_is_ok() {
        assert_eq!(classify_slot_status(StatusCode::OK, ""), SlotIoResult::Ok);
    }

    #[test]
    fn classify_404_is_not_found() {
        assert_eq!(
            classify_slot_status(StatusCode::NOT_FOUND, "no such file"),
            SlotIoResult::NotFound
        );
    }

    /// A 400 (e.g. "Invalid filename") must be terminal — retrying it just
    /// burns the slot semaphore and re-logs. The body must be surfaced.
    #[test]
    fn classify_400_is_permanent_with_body() {
        match classify_slot_status(StatusCode::BAD_REQUEST, "Invalid filename") {
            SlotIoResult::Permanent(msg) => {
                assert!(
                    msg.contains("400"),
                    "message should carry the status: {msg}"
                );
                assert!(
                    msg.contains("Invalid filename"),
                    "message should carry the body: {msg}"
                );
            }
            other => panic!("expected Permanent, got {other:?}"),
        }
    }

    /// A 5xx is a plausibly-transient server condition and stays retryable.
    #[test]
    fn classify_5xx_is_transient() {
        assert!(matches!(
            classify_slot_status(StatusCode::INTERNAL_SERVER_ERROR, "boom"),
            SlotIoResult::Transient(_)
        ));
    }

    /// Clearing all sessions (`None`) must remove every `.bin` file regardless
    /// of model prefix.
    #[tokio::test]
    async fn clear_slot_files_none_clears_every_model() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();

        std::fs::write(slot_bin_path(d, 1, "planner"), b"x").unwrap();
        std::fs::write(slot_bin_path(d, 2, "coder"), b"x").unwrap();

        clear_slot_files(d, None).await.unwrap();

        assert!(!slot_bin_path(d, 1, "planner").exists());
        assert!(!slot_bin_path(d, 2, "coder").exists());
    }

    #[tokio::test]
    async fn test_attempt_save_race_guard() {
        let client = Client::new();
        let clear_all = AtomicBool::new(false);
        let per_session = DashSet::new();

        per_session.insert("planner".to_string());

        // This will return immediately due to the DashSet guard.
        attempt_save(
            &client,
            "http://127.0.0.1:0",
            Path::new("/tmp"),
            0, // model_id
            "planner",
            &clear_all,
            &per_session,
        )
        .await;
    }

    /// The LRU eviction task must exit promptly on cancellation rather than
    /// running forever detached from the server's lifecycle — otherwise it
    /// leaks across proxy restarts within the same process (e.g. tests, or a
    /// GUI that stops/starts the proxy repeatedly).
    #[tokio::test]
    async fn spawn_lru_eviction_task_exits_on_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let handle = spawn_lru_eviction_task(dir.path().to_path_buf(), 10, cancel.clone());

        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("eviction task should exit promptly on cancellation")
            .expect("eviction task should not panic");
    }

    fn unix_secs_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[tokio::test]
    async fn slot_file_is_stale_true_when_file_predates_server_start() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(slot_bin_path(dir.path(), 42, "s"), b"x").unwrap();

        let server_start = unix_secs_now() + 3600; // "started" after the file was written
        assert!(slot_file_is_stale(dir.path(), 42, "s", server_start).await);
    }

    #[tokio::test]
    async fn slot_file_is_stale_false_when_file_is_newer_than_server_start() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(slot_bin_path(dir.path(), 42, "s"), b"x").unwrap();

        let server_start = unix_secs_now().saturating_sub(3600); // started well before the file
        assert!(!slot_file_is_stale(dir.path(), 42, "s", server_start).await);
    }

    #[tokio::test]
    async fn slot_file_is_stale_false_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        // no file written for "s"
        assert!(!slot_file_is_stale(dir.path(), 42, "s", unix_secs_now() + 3600).await);
    }

    #[tokio::test]
    async fn slot_file_is_stale_false_when_guard_uninitialized() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(slot_bin_path(dir.path(), 42, "s"), b"x").unwrap();
        // server_start_secs == 0 means the guard hasn't been initialized yet — fail-open.
        assert!(!slot_file_is_stale(dir.path(), 42, "s", 0).await);
    }
}
