//! The reply text a frame carries.

use serde_json::Value;

/// The content delta of a chat-completion chunk, when it has one. Progress,
/// reasoning and error frames carry none.
pub(super) fn delta_text(frame: &str) -> Option<String> {
    let value: Value = serde_json::from_str(frame).ok()?;
    value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Whether `show` without `--follow` has printed everything the run had
/// logged when it was asked: `last_seq` is that count.
pub(super) const fn past_snapshot(follow: bool, last_seq: u32, seq: u32) -> bool {
    !follow && seq >= last_seq
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_content_delta_is_reply_text() {
        let chunk = r#"{"choices":[{"delta":{"content":"Hi"}}]}"#;
        assert_eq!(delta_text(chunk).as_deref(), Some("Hi"));
        for other in [
            r#"{"choices":[{"delta":{"reasoning_content":"hmm"}}]}"#,
            r#"{"choices":[{"delta":{"content":""}}]}"#,
            r#"{"prompt_progress":{"processed":10,"total":20}}"#,
            r#"{"error":{"message":"boom"}}"#,
            "not json",
        ] {
            assert_eq!(delta_text(other), None, "{other}");
        }
    }

    #[test]
    fn a_snapshot_ends_at_the_last_event_when_asked_and_following_never_does() {
        assert!(!past_snapshot(false, 3, 2));
        assert!(past_snapshot(false, 3, 3));
        assert!(!past_snapshot(true, 3, 3));
        assert!(!past_snapshot(true, 3, 9));
    }
}
