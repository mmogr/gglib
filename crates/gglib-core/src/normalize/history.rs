//! Cross-turn "thinking debt" removal for chat history.
//!
//! Small reasoning models (e.g. Qwen3.5-4B) have a strong tendency to
//! pattern-match their own previous `<think>` traces in the conversation
//! history and produce an unbounded thinking stream that never closes —
//! the model sees prior reasoning trails and tries to extend them. The
//! reference fix, mirroring `OpenAI`'s native behavior, is to drop reasoning
//! artifacts from prior assistant turns before the model sees them.
//!
//! This module is the **single source of truth** for that scrub. Every
//! surface that builds a chat-completion request body must pipe its
//! `messages` array through [`strip_thinking_debt`] so the proxy, the
//! in-process agent loop (CLI / Tauri), and any future direct-mode
//! consumer all benefit equally.
//!
//! The transform is:
//!
//! * Unconditional — there is no per-model gate. Non-reasoning messages
//!   simply have nothing to strip and pass through untouched.
//! * Defensive — only assistant messages are touched; user, system, tool,
//!   and developer messages are never modified.
//! * Conservative on shape — the `reasoning_content` key is removed
//!   outright. String `content` has every `<think>...</think>` block
//!   excised. Non-string `content` (multi-part array form) is left alone.
//! * Forward-safe on unclosed tags — an unclosed `<think>` from the most
//!   recent turn is preserved verbatim; the upstream is responsible for
//!   closing it.

use serde_json::Value;

/// Strip reasoning artifacts from prior assistant messages in `messages`.
///
/// Returns the number of assistant entries that were modified. A return
/// value of `0` means the caller can safely skip any re-serialization
/// step.
///
/// See the module docs for the exact rules.
pub fn strip_thinking_debt(messages: &mut [Value]) -> usize {
    let mut touched = 0usize;
    for msg in messages.iter_mut() {
        let Some(obj) = msg.as_object_mut() else {
            continue;
        };
        let is_assistant = obj
            .get("role")
            .and_then(|r| r.as_str())
            .is_some_and(|r| r == "assistant");
        if !is_assistant {
            continue;
        }

        let removed_reasoning = obj.remove("reasoning_content").is_some();
        let stripped_inline =
            if let Some(Value::String(s)) = obj.get_mut("content") {
                strip_think_blocks(s).is_some_and(|new_s| {
                    *s = new_s;
                    true
                })
            } else {
                false
            };

        if removed_reasoning || stripped_inline {
            touched += 1;
        }
    }
    touched
}

/// Remove every `<think>...</think>` block from `s`.
///
/// Returns `Some(new_string)` when at least one block was removed,
/// otherwise `None` so the caller can avoid a needless allocation.
/// Matching is case-sensitive: each `<think>` is paired with the next
/// `</think>` that follows it. An unclosed `<think>` is left intact (the
/// upstream model is responsible for closing it).
fn strip_think_blocks(s: &str) -> Option<String> {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";

    if !s.contains(OPEN) {
        return None;
    }

    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut changed = false;
    while let Some(open_idx) = rest.find(OPEN) {
        let after_open = &rest[open_idx + OPEN.len()..];
        let Some(close_off) = after_open.find(CLOSE) else {
            // Unclosed <think>: keep verbatim, stop scanning.
            break;
        };
        out.push_str(&rest[..open_idx]);
        rest = &after_open[close_off + CLOSE.len()..];
        changed = true;
    }
    if !changed {
        return None;
    }
    out.push_str(rest);
    Some(out.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msgs(v: Value) -> Vec<Value> {
        match v {
            Value::Array(a) => a,
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn strip_removes_reasoning_content_from_assistant_message() {
        let mut m = msgs(json!([
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello", "reasoning_content": "long ramble..."}
        ]));
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 1);
        assert_eq!(m[1]["content"], "hello");
        assert!(m[1].get("reasoning_content").is_none());
        assert_eq!(m[0]["content"], "hi");
    }

    #[test]
    fn strip_removes_inline_think_blocks_from_assistant_content() {
        let mut m = msgs(json!([
            {"role": "assistant", "content": "<think>secret\nplan</think>The answer is 42."}
        ]));
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 1);
        assert_eq!(m[0]["content"], "The answer is 42.");
    }

    #[test]
    fn strip_handles_multiple_think_blocks() {
        let mut m = msgs(json!([
            {"role": "assistant", "content": "<think>a</think>between<think>b</think>after"}
        ]));
        strip_thinking_debt(&mut m);
        assert_eq!(m[0]["content"], "betweenafter");
    }

    #[test]
    fn strip_leaves_unclosed_think_intact() {
        let mut m = msgs(json!([
            {"role": "assistant", "content": "<think>still going..."}
        ]));
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 0);
        assert_eq!(m[0]["content"], "<think>still going...");
    }

    #[test]
    fn strip_does_not_touch_user_or_system_or_tool_messages() {
        let original = json!([
            {"role": "system", "content": "<think>policy</think>be helpful", "reasoning_content": "x"},
            {"role": "user", "content": "<think>ignore</think>question", "reasoning_content": "y"},
            {"role": "tool", "content": "<think>tool</think>result", "tool_call_id": "c1", "reasoning_content": "z"}
        ]);
        let mut m = msgs(original.clone());
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 0);
        assert_eq!(Value::Array(m), original);
    }

    #[test]
    fn strip_handles_empty_messages_array() {
        let mut m: Vec<Value> = Vec::new();
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 0);
        assert!(m.is_empty());
    }

    #[test]
    fn strip_skips_when_nothing_to_remove() {
        let original = json!([
            {"role": "assistant", "content": "plain answer"}
        ]);
        let mut m = msgs(original.clone());
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 0);
        assert_eq!(Value::Array(m), original);
    }

    #[test]
    fn strip_preserves_non_string_content() {
        // Array-form content (OpenAI multi-part) is left alone; only
        // reasoning_content gets removed.
        let mut m = msgs(json!([
            {
                "role": "assistant",
                "content": [{"type": "text", "text": "<think>x</think>hi"}],
                "reasoning_content": "r"
            }
        ]));
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 1);
        assert!(m[0].get("reasoning_content").is_none());
        assert_eq!(
            m[0]["content"][0]["text"],
            "<think>x</think>hi"
        );
    }

    #[test]
    fn strip_skips_non_object_messages() {
        // Defensive: a stray non-object entry should not panic.
        let mut m = vec![Value::String("garbage".to_string()), json!({
            "role": "assistant",
            "reasoning_content": "drop me"
        })];
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 1);
        assert!(m[1].get("reasoning_content").is_none());
    }

    #[test]
    fn strip_handles_assistant_without_role_string() {
        // role is a number — defensively treat as not assistant.
        let mut m = msgs(json!([
            {"role": 7, "content": "<think>x</think>y", "reasoning_content": "r"}
        ]));
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 0);
        assert_eq!(m[0]["reasoning_content"], "r");
    }

    #[test]
    fn strip_handles_assistant_with_only_inline_think() {
        // reasoning_content absent, but inline <think> present.
        let mut m = msgs(json!([
            {"role": "assistant", "content": "<think>a</think>b"}
        ]));
        let touched = strip_thinking_debt(&mut m);
        assert_eq!(touched, 1);
        assert_eq!(m[0]["content"], "b");
    }
}
