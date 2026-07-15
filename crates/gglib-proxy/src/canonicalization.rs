//! Structural normaliser for chat-completion request bodies.
//!
//! Scans the first system message for IDE-injected dynamic lines (date, time,
//! terminal line count) and extracts them into a separate user message at the
//! end of the array.  This makes the system prompt prefix byte-identical
//! across requests, which stabilises BPE tokenisation for recurrent models
//! that cache KV state per-prefix.

use std::sync::LazyLock;

use bytes::Bytes;
use regex::Regex;
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
}
