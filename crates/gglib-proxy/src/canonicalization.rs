//! Structural normaliser for chat-completion request bodies.
//!
//! IDE clients inject dynamic lines into the system prompt — the current date,
//! the current time, the terminal's line count. They change on essentially
//! every request, and because they sit *inside* the system prompt they break
//! llama.cpp's common-prefix match at the very first tokens, forcing a full
//! re-prefill every turn.
//!
//! This module stabilises them in place: the line count is dropped, the time is
//! rounded down to the hour, and the date is left alone. The prompt then stays
//! byte-identical for an hour at a stretch.
//!
//! # Why not extract them
//!
//! It used to move the lines into a `user` message appended after the
//! conversation. That kept the prefix stable but put a synthetic turn in the
//! position with the most attention — the model read `Current date: …` as the
//! last thing before generating, instead of the user's actual instruction.
//!
//! It also did nothing at all for the client that motivated it: the extraction
//! required string-form `content`, and the VS Code LLM Gateway sends the
//! array-form the `OpenAI` spec allows, so the whole pass early-returned and
//! the volatile lines stayed in the prompt. Stabilising in place fixes both,
//! and handles either content shape.

use std::sync::LazyLock;

use bytes::Bytes;
use gglib_core::domain::ChatMessage;
use regex::Regex;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

/// Environment kill switch. Truthy values (case-insensitive `1`, `true`,
/// `yes`, `on`) leave the system prompt exactly as the client sent it — the
/// same contract as `GGLIB_DISABLE_GRAMMAR` and
/// `GGLIB_DISABLE_AGENTIC_SAMPLING`.
///
/// An environment variable rather than a setting because this runs as the very
/// first statement of the request handler, before the settings snapshot is
/// read, and the session-id derivation downstream depends on it having already
/// happened.
pub const DISABLE_CANONICALIZATION_ENV: &str = "GGLIB_DISABLE_PROMPT_CANONICALIZATION";

/// Whether [`DISABLE_CANONICALIZATION_ENV`] is set to a truthy value.
fn canonicalization_disabled_via_env() -> bool {
    gglib_core::debug_switches::enabled(DISABLE_CANONICALIZATION_ENV)
}

/// Matches dynamic IDE-injected lines at the start of a line (multiline mode).
///
/// The pattern captures the trailing newline (`\r?\n`) so `replace_all` removes
/// the entire line including its line ending.  Without consuming the newline a
/// matched line in the middle of the prompt would leave a double `\n\n`.
static DYNAMIC_LINE_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(Current date:|Current time:|Current terminal line count)[^\n]*(?:\r?\n|$)")
        .expect("hardcoded regex should always compile")
});

/// Matches a whole `Current terminal line count …` line, newline included.
///
/// Dropped rather than coarsened: it changes as the user scrolls, has no
/// bucket that would hold still, and is of no use to a coding model.
static LINE_COUNT_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^Current terminal line count[^\n]*(?:\r?\n|$)")
        .expect("hardcoded regex should always compile")
});

/// Matches a clock time inside a `Current time:` line — `HH:MM`, optionally
/// with seconds. Deliberately loose about what surrounds it so a 12-hour
/// format with a trailing meridiem still matches.
static CLOCK_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{1,2}):(\d{2})(:\d{2})?").expect("hardcoded regex should always compile")
});

/// Matches a whole `Current time: …` line, capturing it for rewriting.
static TIME_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^Current time:[^\n]*").expect("hardcoded regex should always compile")
});

/// Round every clock time in the text down to the hour, and drop the terminal
/// line count entirely. Returns `None` when nothing needed changing.
///
/// `Current date:` is deliberately untouched. It changes once a day, which is
/// slow enough not to matter, and the date is genuinely useful to the model.
///
/// A `Current time:` line with no recognisable clock in it is removed rather
/// than left alone: it cannot be coarsened, and leaving it would defeat the
/// point.
fn stabilise_dynamic_lines(text: &str) -> Option<String> {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");

    let without_line_count = LINE_COUNT_LINE.replace_all(&normalised, "");

    let stabilised = TIME_LINE.replace_all(&without_line_count, |caps: &regex::Captures| {
        let line = &caps[0];
        CLOCK_TIME.find(line).map_or_else(
            || {
                debug!("canonicalise: dropping a Current time line with no recognisable clock");
                String::new()
            },
            |m| {
                let hour = CLOCK_TIME
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .map_or("0", |h| h.as_str());
                format!("{}{hour}:00{}", &line[..m.start()], &line[m.end()..])
            },
        )
    });

    (stabilised != text).then(|| stabilised.into_owned())
}

/// Stabilise the dynamic IDE-injected lines in the first system message.
///
/// Step 0 of the request pipeline. Rewrites the system prompt in place so it
/// stops changing between requests, which is what lets llama.cpp match a
/// common prefix and skip re-prefilling it.
///
/// # Algorithm
///
/// 1. Parse body as JSON — return unchanged on parse failure.
/// 2. Locate the `"messages"` array — return unchanged if absent.
/// 3. Find the first `"role": "system"` message, whatever shape its
///    `"content"` takes.
/// 4. Rewrite every text run in that content: drop the terminal line count,
///    round any clock time down to the hour, leave the date alone.
///
/// # Both content shapes
///
/// `content` may be a plain string or the `OpenAI` array-of-parts form. Both
/// are handled, and an array keeps its shape — only the `text` inside each
/// part is rewritten. Handling only the string form is what made the previous
/// version a no-op for the VS Code LLM Gateway.
///
/// # Fail-open
///
/// On any parse or serialisation failure the original `Bytes` are returned
/// unchanged — zero blast radius for unexpected request shapes.
pub fn canonicalize_system_prompt(body: Bytes) -> Bytes {
    canonicalize_system_prompt_with(body, canonicalization_disabled_via_env())
}

/// [`canonicalize_system_prompt`] with the environment override supplied
/// explicitly.
///
/// Split out so the behaviour itself is testable without mutating process-wide
/// environment state (this crate denies `unsafe`, which `set_var` requires) —
/// the same split `resolve_slot_restore` uses for its own env override.
fn canonicalize_system_prompt_with(body: Bytes, disabled: bool) -> Bytes {
    if disabled {
        return body;
    }

    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };

    let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return body;
    };

    // The first system message, regardless of content shape.
    let Some(sys_idx) = messages.iter().position(|m| {
        m.get("role")
            .and_then(|r| r.as_str())
            .is_some_and(|r| r == "system")
    }) else {
        return body;
    };

    let Some(content) = messages[sys_idx].get_mut("content") else {
        return body;
    };

    let changed = match content {
        serde_json::Value::String(text) => stabilise_dynamic_lines(text)
            .map(|stabilised| {
                *text = stabilised;
            })
            .is_some(),
        // Array-of-parts: rewrite the text inside each part, keep the shape.
        serde_json::Value::Array(parts) => {
            let mut any = false;
            for part in parts.iter_mut() {
                if let Some(serde_json::Value::String(text)) = part.get_mut("text")
                    && let Some(stabilised) = stabilise_dynamic_lines(text)
                {
                    *text = stabilised;
                    any = true;
                }
            }
            any
        }
        _ => false,
    };

    if !changed {
        return body;
    }

    debug!("canonicalised system prompt: stabilised dynamic lines in place");

    match serde_json::to_vec(&value) {
        Ok(v) => Bytes::from(v),
        Err(e) => {
            warn!(error = %e, "failed to re-serialize after canonicalisation; forwarding original");
            body
        }
    }
}

/// Canonicalise the `tools[]` array into a stable, deterministic order.
///
/// llama.cpp's Jinja template renders tool/function schemas early in the
/// prompt, right after the system message (see [`log_tool_names_for_diagnostics`]
/// for how this was diagnosed). If the calling client sends `tools[]` in a
/// different order between two turns of the same conversation, those early
/// tokens change and llama.cpp's common-prefix match breaks for everything
/// after — a full cold re-prefill even though the conversation didn't
/// meaningfully change. Sorting by `function.name` before forwarding makes
/// gglib's own request byte-stable regardless of what order the client sent,
/// independent of genuine membership changes (adding/removing a tool), which
/// remain a real client-side change this function cannot and must not hide.
///
/// # Sort key
///
/// `tools[].function.name`, ascending. A **stable** sort, so entries sharing
/// a key — including any missing `function.name`, which sorts first as
/// `None` — keep their relative order rather than being shuffled arbitrarily.
///
/// # Fail-open
///
/// No `tools` array, fewer than two entries, or a re-serialization failure
/// all return the original `Bytes` unchanged.
pub fn canonicalize_tool_order(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };

    let Some(tools) = value.get_mut("tools").and_then(|v| v.as_array_mut()) else {
        return body;
    };

    if tools.len() < 2 {
        return body;
    }

    let already_sorted = tools
        .windows(2)
        .all(|w| tool_name(&w[0]) <= tool_name(&w[1]));
    if already_sorted {
        return body;
    }

    tools.sort_by(|a, b| tool_name(a).cmp(&tool_name(b)));
    debug!(
        tool_count = tools.len(),
        "canonicalised tools[] order for cache prefix stability"
    );

    match serde_json::to_vec(&value) {
        Ok(v) => Bytes::from(v),
        Err(e) => {
            warn!(error = %e, "failed to re-serialize after tool-order canonicalisation; forwarding original");
            body
        }
    }
}

/// `tools[N].function.name`, or `None` for a malformed entry. `Option<&str>`
/// sorts `None` first — deterministic, never panics.
fn tool_name(tool: &serde_json::Value) -> Option<&str> {
    tool.get("function")?.get("name")?.as_str()
}

/// Number of leading digest bytes kept in [`derive_fallback_session_id`]'s
/// identifier (16 bytes = 128 bits — ample collision resistance for a cache
/// bucketing key that only needs fail-open behaviour on collision, not
/// cryptographic guarantees).
const FALLBACK_ID_DIGEST_BYTES: usize = 16;

/// Derive a stable, content-based session identifier for KV cache
/// save/restore when the caller did not supply an `X-Gglib-Session-Id`
/// header.
///
/// Hashes the system prompt together with the first user message. Both are
/// stable for the entire life of one agent's conversation: `truncate_history`
/// (see `truncation.rs`) never modifies `system` messages or `user`-role
/// content, so this fingerprint doesn't drift as history grows. Different
/// agents (different system prompt) or different task instances of the same
/// agent (different first user message) land in different buckets without
/// any client cooperation.
///
/// # Preconditions
///
/// None. This used to require pre-canonicalized input, because it hashed the
/// system prompt verbatim and would otherwise have fingerprinted the clock.
/// It now strips the dynamic lines itself, so the id is stable whether or not
/// canonicalisation ran — including when it is switched off via
/// [`DISABLE_CANONICALIZATION_ENV`], which previously would have rotated the
/// session id on every request.
///
/// Returns `None` when the body has no usable `messages` array, or neither
/// a system nor a first user message is present — callers should treat that
/// the same as "no session available".
///
/// # Fail-open
///
/// A hash collision (two distinct conversations sharing an identical system
/// prompt *and* identical first user message) just means one restores the
/// other's cache; llama-server still re-syncs against whatever prefix
/// actually matches the incoming prompt, so the worst case is a wasted
/// restore/save, never a wrong answer.
pub fn derive_fallback_session_id(body: &Bytes) -> Option<String> {
    let mut value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let messages_raw = value.get_mut("messages")?.take();
    let messages: Vec<ChatMessage> = serde_json::from_value(messages_raw).ok()?;

    // Every dynamic line is stripped before hashing, not just coarsened.
    // `canonicalize_system_prompt` rounds the clock to the hour and leaves the
    // date alone, both of which still turn over eventually — and a fingerprint
    // that turns over costs the session its KV slot and its
    // `TokenCalibration` snapshot at the boundary. Removing them here
    // decouples the identity of a conversation from the clock entirely.
    let system_text = messages
        .iter()
        .find(|m| m.role == "system")
        .and_then(|m| m.content.clone())
        .map(|c| {
            DYNAMIC_LINE_PATTERNS
                .replace_all(&c.into_string(), "")
                .into_owned()
        })
        .unwrap_or_default();

    let first_user_text = messages
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.clone())
        .map(|c| c.into_string())
        .unwrap_or_default();

    if system_text.is_empty() && first_user_text.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(system_text.as_bytes());
    // Separator byte so ("ab", "c") and ("a", "bc") don't collide.
    hasher.update([0u8]);
    hasher.update(first_user_text.as_bytes());
    let digest = hasher.finalize();

    let hex: String = digest[..FALLBACK_ID_DIGEST_BYTES]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Some(format!("auto-{hex}"))
}

/// Diagnostic only: log the request's `tools` array — function names in
/// their original order — tagged with the resolved cache session id.
///
/// KV cache restores land near the front of the prompt (tool/function
/// schemas are typically enumerated early), so when a restore's LCP
/// similarity comes back low for a session that should be stable, the
/// question is whether the *client* changed the tool list shape between
/// turns rather than anything gglib did. Since [`canonicalize_tool_order`]
/// now runs before this (see the call site in `chat_completions`), *order*
/// drift is no longer a possible answer — it's structurally eliminated
/// upstream. What's left for this log to diagnose is *membership* drift:
/// comparing two consecutive log lines for the same session_id, identical
/// list → not the cause; different names → a real client-side change
/// (a tool added/removed), outside the proxy's control.
///
/// A no-op (skips the parse entirely) unless DEBUG-level tracing is
/// actually enabled, so this costs nothing outside `-v` investigations.
/// Fail-open: any parse failure or missing `tools` field is simply not
/// logged, never an error.
pub fn log_tool_names_for_diagnostics(body: &Bytes, session_id: &str) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    let Some(names) = extract_tool_names(body) else {
        return;
    };
    debug!(
        session_id,
        tool_count = names.len(),
        tools = ?names,
        "tool list for cache diagnostics"
    );
}

/// Extract `tools[].function.name` from a request body, in original order.
/// `None` if the body doesn't parse as JSON or carries no `tools` array.
fn extract_tool_names(body: &Bytes) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let tools = value.get("tools")?.as_array()?;
    Some(
        tools
            .iter()
            .filter_map(|t| t.get("function")?.get("name")?.as_str().map(String::from))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stabilises_dynamic_lines_in_place() {
        let body = Bytes::from(
            serde_json::json!({"messages": [
                {"role": "system", "content": "You are an assistant.\nCurrent date: 2026-07-15\nCurrent time: 10:30\nCurrent terminal line count: 42\nMore instructions."},
                {"role": "user", "content": "hello"},
            ]})
            .to_string(),
        );

        let out = canonicalize_system_prompt(body);
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let messages = value["messages"].as_array().unwrap();
        let system = messages[0]["content"].as_str().unwrap();

        // No synthetic turn: the user's message is still the last one.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["content"], "hello");

        // The date survives verbatim — it turns over once a day.
        assert!(system.contains("Current date: 2026-07-15"), "{system}");
        // The clock is rounded down to the hour.
        assert!(system.contains("Current time: 10:00"), "{system}");
        // The line count is gone entirely.
        assert!(!system.contains("terminal line count"), "{system}");
        // Surrounding instructions are untouched, with no torn blank line.
        assert!(system.starts_with("You are an assistant."), "{system}");
        assert!(system.ends_with("More instructions."), "{system}");
        assert!(!system.contains("\n\n"), "{system}");
    }

    /// Two requests within the same hour must produce a byte-identical system
    /// prompt — that is the whole point, and what lets the prefix match.
    #[test]
    fn requests_within_the_hour_are_byte_identical() {
        let at = |clock: &str, lines: u32| {
            Bytes::from(
                serde_json::json!({"messages": [
                    {"role": "system", "content": format!("Preamble.\nCurrent date: 2026-07-15\nCurrent time: {clock}\nCurrent terminal line count: {lines}")},
                    {"role": "user", "content": "hi"},
                ]})
                .to_string(),
            )
        };

        let first = canonicalize_system_prompt(at("10:03:11", 12));
        let second = canonicalize_system_prompt(at("10:57:48", 300));

        assert_eq!(first, second);
    }

    /// The shape the VS Code LLM Gateway actually sends. The previous
    /// implementation required string content and silently did nothing here.
    #[test]
    fn array_form_system_content_is_stabilised() {
        let body = Bytes::from(
            serde_json::json!({"messages": [
                {"role": "system", "content": [
                    {"type": "text", "text": "You are an assistant.\nCurrent time: 14:32\nCurrent terminal line count: 7"},
                ]},
                {"role": "user", "content": "hello"},
            ]})
            .to_string(),
        );

        let out = canonicalize_system_prompt(body);
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let parts = value["messages"][0]["content"].as_array().unwrap();

        // The array shape is preserved; only the text inside is rewritten.
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["type"], "text");
        let text = parts[0]["text"].as_str().unwrap();
        assert!(text.contains("Current time: 14:00"), "{text}");
        assert!(!text.contains("terminal line count"), "{text}");
    }

    /// A time we cannot read cannot be coarsened, so it is dropped rather than
    /// left to churn the prefix.
    #[test]
    fn an_unparseable_time_line_is_dropped() {
        let body = Bytes::from(
            serde_json::json!({"messages": [
                {"role": "system", "content": "Preamble.\nCurrent time: just after tea\nRest."},
            ]})
            .to_string(),
        );

        let out = canonicalize_system_prompt(body);
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let system = value["messages"][0]["content"].as_str().unwrap();

        assert!(!system.contains("Current time"), "{system}");
        assert!(system.contains("Preamble."), "{system}");
        assert!(system.contains("Rest."), "{system}");
    }

    /// A 12-hour clock keeps its meridiem.
    #[test]
    fn a_twelve_hour_clock_keeps_its_suffix() {
        let body = Bytes::from(
            serde_json::json!({"messages": [
                {"role": "system", "content": "Current time: 2:05 PM"},
            ]})
            .to_string(),
        );

        let out = canonicalize_system_prompt(body);
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["messages"][0]["content"], "Current time: 2:00 PM");
    }

    #[test]
    fn the_kill_switch_leaves_the_prompt_byte_identical() {
        let body = Bytes::from(
            serde_json::json!({"messages": [
                {"role": "system", "content": "Current time: 10:30"},
            ]})
            .to_string(),
        );

        assert_eq!(
            canonicalize_system_prompt_with(body.clone(), true),
            body,
            "a disabled pass must not even re-serialize"
        );
        assert_ne!(canonicalize_system_prompt_with(body.clone(), false), body);
    }

    #[test]
    fn no_dynamic_lines_unchanged() {
        let body = serde_json::to_vec(&serde_json::json!({
            "messages": [{"role": "system", "content": "Just a normal prompt."}, {"role": "user", "content": "Hi"}]
        }))
        .unwrap();
        let result = canonicalize_system_prompt(Bytes::from(body.clone()));
        assert_eq!(result.as_ref(), body.as_slice()); // Byte-identical
    }

    #[test]
    fn invalid_json_passthrough() {
        let body = Bytes::from(b"not json".to_vec());
        let result = canonicalize_system_prompt(body.clone());
        assert_eq!(result, body);
    }

    fn body_with(system: &str, user: &str) -> Bytes {
        Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user}
                ]
            }))
            .unwrap(),
        )
    }

    #[test]
    fn fallback_session_id_stable_across_turns() {
        let turn1 = body_with("You are the Planner.", "Design a login flow");
        let turn2 = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "messages": [
                    {"role": "system", "content": "You are the Planner."},
                    {"role": "user", "content": "Design a login flow"},
                    {"role": "assistant", "content": "Here's a plan..."},
                    {"role": "user", "content": "Now refine step 2"}
                ]
            }))
            .unwrap(),
        );
        let id1 = derive_fallback_session_id(&turn1).unwrap();
        let id2 = derive_fallback_session_id(&turn2).unwrap();
        assert_eq!(
            id1, id2,
            "same agent/task should map to the same bucket across turns"
        );
    }

    #[test]
    fn fallback_session_id_differs_by_role() {
        let planner = body_with("You are the Planner.", "Design a login flow");
        let coder = body_with("You are the Coder.", "Design a login flow");
        assert_ne!(
            derive_fallback_session_id(&planner).unwrap(),
            derive_fallback_session_id(&coder).unwrap()
        );
    }

    #[test]
    fn fallback_session_id_differs_by_task() {
        let task_a = body_with("You are the Coder.", "Implement login");
        let task_b = body_with("You are the Coder.", "Implement logout");
        assert_ne!(
            derive_fallback_session_id(&task_a).unwrap(),
            derive_fallback_session_id(&task_b).unwrap()
        );
    }

    #[test]
    fn fallback_session_id_ignores_dynamic_lines() {
        // derive_fallback_session_id requires pre-canonicalized input (see its
        // doc comment) — the caller (chat_completions) canonicalizes once up
        // front. Mirror that contract here rather than passing raw bodies.
        let with_timestamp = canonicalize_system_prompt(body_with(
            "You are an assistant.\nCurrent date: 2026-07-15\nMore instructions.",
            "Hello",
        ));
        let without_timestamp = canonicalize_system_prompt(body_with(
            "You are an assistant.\nMore instructions.",
            "Hello",
        ));
        assert_eq!(
            derive_fallback_session_id(&with_timestamp).unwrap(),
            derive_fallback_session_id(&without_timestamp).unwrap(),
            "dynamic IDE-injected lines must not change the fingerprint turn to turn"
        );
    }

    #[test]
    fn fallback_session_id_handles_array_form_content() {
        let string_form = body_with("You are the Coder.", "Implement login");
        let array_form = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "messages": [
                    {"role": "system", "content": [{"type": "text", "text": "You are the Coder."}]},
                    {"role": "user", "content": [{"type": "text", "text": "Implement login"}]}
                ]
            }))
            .unwrap(),
        );
        assert_eq!(
            derive_fallback_session_id(&string_form).unwrap(),
            derive_fallback_session_id(&array_form).unwrap(),
            "string and array content forms carrying the same text must fingerprint identically"
        );
    }

    #[test]
    fn fallback_session_id_none_without_messages() {
        let body = Bytes::from(serde_json::to_vec(&serde_json::json!({"foo": "bar"})).unwrap());
        assert!(derive_fallback_session_id(&body).is_none());
    }

    #[test]
    fn fallback_session_id_none_on_invalid_json() {
        let body = Bytes::from(b"not json".to_vec());
        assert!(derive_fallback_session_id(&body).is_none());
    }

    #[test]
    fn fallback_session_id_is_valid_for_sanitize() {
        let body = body_with("You are the Coder.", "Implement login");
        let id = derive_fallback_session_id(&body).unwrap();
        crate::slots::sanitize_session_id(&id).expect("derived id must pass sanitize_session_id");
    }

    #[test]
    fn extract_tool_names_preserves_original_order() {
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "messages": [],
                "tools": [
                    {"type": "function", "function": {"name": "create_branch"}},
                    {"type": "function", "function": {"name": "create_pull_request"}},
                    {"type": "function", "function": {"name": "read_file"}}
                ]
            }))
            .unwrap(),
        );
        assert_eq!(
            extract_tool_names(&body).unwrap(),
            vec!["create_branch", "create_pull_request", "read_file"]
        );
    }

    #[test]
    fn extract_tool_names_none_without_tools_array() {
        let body = Bytes::from(serde_json::to_vec(&serde_json::json!({"messages": []})).unwrap());
        assert!(extract_tool_names(&body).is_none());
    }

    #[test]
    fn extract_tool_names_none_on_invalid_json() {
        let body = Bytes::from(b"not json".to_vec());
        assert!(extract_tool_names(&body).is_none());
    }

    #[test]
    fn extract_tool_names_skips_malformed_entries_without_panicking() {
        // A tool missing `function.name` (or `function` entirely) must be
        // skipped, not crash the whole extraction.
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "tools": [
                    {"type": "function", "function": {"name": "read_file"}},
                    {"type": "function", "function": {}},
                    {"type": "function"},
                    "not even an object"
                ]
            }))
            .unwrap(),
        );
        assert_eq!(extract_tool_names(&body).unwrap(), vec!["read_file"]);
    }

    #[test]
    fn canonicalize_tool_order_sorts_by_function_name() {
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "messages": [],
                "tools": [
                    {"type": "function", "function": {"name": "create_pull_request"}},
                    {"type": "function", "function": {"name": "create_branch"}},
                    {"type": "function", "function": {"name": "read_file"}}
                ]
            }))
            .unwrap(),
        );
        let result = canonicalize_tool_order(body);
        let value: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let names: Vec<&str> = value["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["create_branch", "create_pull_request", "read_file"]
        );
    }

    #[test]
    fn canonicalize_tool_order_is_idempotent_across_two_differently_ordered_turns() {
        // The actual bug this fixes: two turns sending the same set in
        // different order must forward byte-identically past the tools[]
        // boundary, or llama.cpp's common-prefix match breaks right there.
        let turn1 = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "tools": [
                    {"type": "function", "function": {"name": "b_tool"}},
                    {"type": "function", "function": {"name": "a_tool"}}
                ]
            }))
            .unwrap(),
        );
        let turn2 = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "tools": [
                    {"type": "function", "function": {"name": "a_tool"}},
                    {"type": "function", "function": {"name": "b_tool"}}
                ]
            }))
            .unwrap(),
        );
        assert_eq!(
            canonicalize_tool_order(turn1),
            canonicalize_tool_order(turn2)
        );
    }

    #[test]
    fn canonicalize_tool_order_already_sorted_is_byte_identical() {
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "tools": [
                    {"type": "function", "function": {"name": "a"}},
                    {"type": "function", "function": {"name": "b"}}
                ]
            }))
            .unwrap(),
        );
        assert_eq!(canonicalize_tool_order(body.clone()), body);
    }

    #[test]
    fn canonicalize_tool_order_no_tools_field_unchanged() {
        let body = Bytes::from(serde_json::to_vec(&serde_json::json!({"messages": []})).unwrap());
        assert_eq!(canonicalize_tool_order(body.clone()), body);
    }

    #[test]
    fn canonicalize_tool_order_single_tool_unchanged() {
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "tools": [{"type": "function", "function": {"name": "only_one"}}]
            }))
            .unwrap(),
        );
        assert_eq!(canonicalize_tool_order(body.clone()), body);
    }

    #[test]
    fn canonicalize_tool_order_malformed_entries_never_panic() {
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "tools": [
                    {"type": "function", "function": {"name": "z_tool"}},
                    {"type": "function", "function": {}},
                    "not even an object"
                ]
            }))
            .unwrap(),
        );
        let result = canonicalize_tool_order(body); // must not panic
        let _value: serde_json::Value = serde_json::from_slice(&result).unwrap();
    }

    #[test]
    fn canonicalize_tool_order_invalid_json_passthrough() {
        let body = Bytes::from(b"not json".to_vec());
        assert_eq!(canonicalize_tool_order(body.clone()), body);
    }
}
