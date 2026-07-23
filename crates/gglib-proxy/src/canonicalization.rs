//! Structural normaliser for chat-completion request bodies.
//!
//! Scans the first system message for IDE-injected dynamic lines (date, time,
//! terminal line count) and extracts them into a separate user message at the
//! end of the array.  This makes the system prompt prefix byte-identical
//! across requests, which stabilises BPE tokenisation for recurrent models
//! that cache KV state per-prefix.

use std::sync::LazyLock;

use bytes::Bytes;
use gglib_core::domain::ChatMessage;
use regex::Regex;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

/// Matches dynamic IDE-injected lines at the start of a line (multiline mode).
///
/// The pattern captures the trailing newline (`\r?\n`) so `replace_all` removes
/// the entire line including its line ending.  Without consuming the newline a
/// matched line in the middle of the prompt would leave a double `\n\n`.
static DYNAMIC_LINE_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(Current date:|Current time:|Current terminal line count)[^\n]*(?:\r?\n|$)")
        .expect("hardcoded regex should always compile")
});

/// Canonicalise the system prompt by extracting dynamic IDE-injected lines.
///
/// This transform (Step 0 in the request pipeline) ensures the system prompt
/// prefix is byte-identical across requests for BPE stability.
///
/// # Algorithm
///
/// 1. Parse body as JSON — return unchanged on parse failure.
/// 2. Locate the `"messages"` array — return unchanged if absent.
/// 3. Find the first message with `"role": "system"` and string `"content"`.
/// 4. Extract all matching dynamic lines into a new user message appended at
///    the end of the messages array.
/// 5. Remove matched lines from the system prompt content.
///
/// # Fail-open
///
/// On any parse or serialisation failure the original `Bytes` are returned
/// unchanged — zero blast radius for unexpected request shapes.
pub fn canonicalize_system_prompt(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };

    let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return body;
    };

    // Find the first system message with string content.
    let Some(sys_idx) = messages.iter().position(|m| {
        m.get("role")
            .and_then(|r| r.as_str())
            .is_some_and(|r| r == "system")
            && m.get("content").and_then(|c| c.as_str()).is_some()
    }) else {
        return body;
    };

    let original_content = messages[sys_idx]["content"].as_str().unwrap().to_string();

    // Normalize line endings for cross-platform BPE prefix consistency.
    let original_content = original_content.replace("\r\n", "\n").replace("\r", "\n");

    // Collect matched dynamic lines (trimmed to remove trailing newlines the
    // regex captures).
    let extracted_lines: Vec<&str> = DYNAMIC_LINE_PATTERNS
        .captures_iter(&original_content)
        .filter_map(|cap| {
            cap.get(0).and_then(|m| {
                let trimmed = m.as_str().trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
        })
        .collect();

    // Remove all matched lines from the system prompt content.
    let new_content = DYNAMIC_LINE_PATTERNS
        .replace_all(&original_content, "")
        .trim()
        .to_string();

    if new_content.is_empty() {
        // If the entire system content was dynamic lines, remove the key entirely.
        if let Some(obj) = messages[sys_idx].as_object_mut() {
            obj.remove("content");
        }
    } else {
        messages[sys_idx]["content"] = serde_json::Value::String(new_content);
    }

    // Append extracted lines as a new user message at the end.
    if !extracted_lines.is_empty() {
        let joined = extracted_lines.join("\n");
        messages.push(serde_json::json!({
            "role": "user",
            "content": joined,
        }));
        debug!(
            lines = extracted_lines.len(),
            "canonicalised system prompt: extracted dynamic lines"
        );
    }

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
/// `body` must already be canonicalized (see [`canonicalize_system_prompt`]).
/// This function does not canonicalize its input itself — the caller
/// (`chat_completions`) canonicalizes once up front and reuses the result
/// both for this hash and for the forwarded request, rather than paying the
/// parse/regex/serialize cost twice on every request.
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

    let system_text = messages
        .iter()
        .find(|m| m.role == "system")
        .and_then(|m| m.content.clone())
        .map(|c| c.into_string())
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
    fn extracts_all_three_dynamic_lines() {
        let body = serde_json::to_vec(&serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are an assistant.\nCurrent date: 2026-07-15\nCurrent time: 10:30\nCurrent terminal line count: 42\nMore instructions."},
                {"role": "user", "content": "Hello"}
            ]
        }))
        .unwrap();
        let result = canonicalize_system_prompt(Bytes::from(body));
        let value: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let messages = value["messages"].as_array().unwrap();
        // System prompt should NOT contain dynamic lines
        let system_content = &messages[0]["content"];
        assert!(
            system_content
                .as_str()
                .unwrap()
                .contains("You are an assistant")
        );
        assert!(
            system_content
                .as_str()
                .unwrap()
                .contains("More instructions")
        );
        assert!(!system_content.as_str().unwrap().contains("Current date"));
        assert!(!system_content.as_str().unwrap().contains("Current time"));
        assert!(
            !system_content
                .as_str()
                .unwrap()
                .contains("terminal line count")
        );
        // Last message should be the extracted dynamic lines as user message
        assert_eq!(messages.len(), 3); // system + original user + extracted
        assert_eq!(messages[2]["role"], "user");
        assert!(
            messages[2]["content"]
                .as_str()
                .unwrap()
                .contains("Current date: 2026-07-15")
        );
        // No dangling blank lines in cleaned system prompt (regex must consume \n)
        assert!(!system_content.as_str().unwrap().contains("\n\n"));
    }

    #[test]
    fn handles_crlf_line_endings() {
        let body = serde_json::to_vec(&serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are an assistant.\r\nCurrent date: 2026-07-15\r\nMore instructions."},
                {"role": "user", "content": "Hello"}
            ]
        }))
        .unwrap();
        let result = canonicalize_system_prompt(Bytes::from(body));
        let value: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let system_content = &value["messages"][0]["content"];
        assert!(!system_content.as_str().unwrap().contains("Current date"));
        // No dangling blank lines (regex must consume \r\n)
        assert!(!system_content.as_str().unwrap().contains("\n\n"));
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
