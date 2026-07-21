//! Stage 1–2: shaping the conversation itself.
//!
//! Both transforms rewrite the `messages` array and nothing else. They are
//! paired in [`shape_messages`] because they are the contiguous run of
//! message-level work in the pipeline — see [`super::apply`] for why the order
//! within that run is fixed.

use serde_json::Value;
use tracing::{Level, debug, enabled, warn};

use super::ModelContext;
use crate::domain::{ChatMessage, ModelCapabilities, transform_messages_for_capabilities};
use crate::normalize::strip_thinking_debt;

/// Apply every message-level transform, in order.
///
/// Returns `true` when `body` was modified. Callers holding the request as
/// bytes use this to skip a re-serialization and forward the client's original
/// payload untouched — which is not merely an optimisation for the proxy,
/// where history truncation measures the payload in **wire bytes** and would
/// otherwise measure a re-encoded body instead of the one the client sent.
pub fn shape_messages(body: &mut Value, ctx: &ModelContext) -> bool {
    // Evaluated eagerly, not short-circuited: coalescing must run even when
    // the reasoning strip found nothing to do.
    let stripped = strip_prior_reasoning(body);
    let coalesced = coalesce_for_capabilities(body, ctx.capabilities);
    stripped || coalesced
}

/// Stage 1 — scrub reasoning artefacts from prior assistant turns.
///
/// Thin wrapper over [`strip_thinking_debt`], which owns the scrub rules; this
/// exists only to locate the `messages` array and report whether anything
/// changed. Reasoning models pattern-match their own past `<think>` traces
/// when these are left in the history.
fn strip_prior_reasoning(body: &mut Value) -> bool {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };

    let touched = strip_thinking_debt(messages);
    if touched > 0 {
        debug!(touched, "stripped prior reasoning from assistant messages");
    }
    touched > 0
}

/// Stage 2 — merge consecutive same-role messages for strict-turn models.
///
/// Mistral-family models (and anything else carrying
/// [`ModelCapabilities::REQUIRES_STRICT_TURNS`]) enforce user/assistant
/// alternation inside their Jinja chat templates and raise a hard 500 when
/// consecutive same-role messages arrive. IDEs and gateway extensions routinely
/// send multi-turn context that violates this, so coalescing here is the
/// correct fix rather than constraining callers.
///
/// [`transform_messages_for_capabilities`] is the single source of truth for
/// the merging rules. Returns `false` — leaving `body` untouched — for any
/// model that needs no rewriting and for any request whose `messages` array
/// cannot be read.
fn coalesce_for_capabilities(body: &mut Value, capabilities: ModelCapabilities) -> bool {
    // Fast paths: this model needs no rewriting, or we know nothing about it.
    let needs_nothing =
        !capabilities.requires_strict_turns() && capabilities.supports_system_role();
    if needs_nothing || capabilities.is_empty() {
        return false;
    }

    debug!(
        requires_strict_turns = capabilities.requires_strict_turns(),
        supports_system_role = capabilities.supports_system_role(),
        "coalesce: entering message transformation"
    );

    let Some(messages_raw) = body.get("messages").and_then(Value::as_array) else {
        debug!("coalesce: no messages array found in request body");
        return false;
    };

    let before_count = messages_raw.len();

    // Deserialise only the fields `transform_messages_for_capabilities` needs.
    // `ChatMessage.content` accepts both a plain JSON string and a JSON array of
    // content-part objects (e.g. VSCode LLM Gateway sends array-form content per
    // the OpenAI spec), and every other key rides along in `ChatMessage.extra`,
    // so this round-trip is lossless.
    let messages: Vec<ChatMessage> =
        match serde_json::from_value(Value::Array(messages_raw.clone())) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    error = %e,
                    before = before_count,
                    "coalesce: failed to deserialise messages as Vec<ChatMessage>; \
                     leaving the message array unchanged. \
                     This usually means a message field has an unexpected type."
                );
                return false;
            }
        };

    log_payload_shape(body, &messages, before_count);

    let transformed = transform_messages_for_capabilities(messages, capabilities);
    let after_count = transformed.len();

    debug!(
        before = before_count,
        after = after_count,
        merged = before_count.saturating_sub(after_count),
        "coalesce: transformation complete"
    );

    match serde_json::to_value(&transformed) {
        Ok(new_messages) => {
            body["messages"] = new_messages;
            true
        }
        Err(e) => {
            warn!(error = %e, "coalesce: failed to serialise transformed messages; leaving them unchanged");
            false
        }
    }
}

/// Diagnostics for tracking down oversized payloads: the byte cost of every
/// non-message top-level field, and the content size of every message.
///
/// Gated on the log level as a whole because both loops serialise their way to
/// a size — work the `debug!` macro would otherwise discard *after* paying for
/// it on every strict-turn request.
fn log_payload_shape(body: &Value, messages: &[ChatMessage], before_count: usize) {
    if !enabled!(Level::DEBUG) {
        return;
    }

    for (key, val) in body.as_object().into_iter().flatten() {
        if key != "messages" {
            let approx_bytes = serde_json::to_vec(val).map_or(0, |v| v.len());
            debug!(key, approx_bytes, "coalesce: top-level field size");
        }
    }

    debug!(
        before = before_count,
        roles = ?messages.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(),
        "coalesce: parsed messages for transformation"
    );
    for (i, m) in messages.iter().enumerate() {
        let content_bytes = m.content.as_ref().map_or(0, |c| {
            c.as_str().map_or_else(|| format!("{c:?}").len(), str::len)
        });
        debug!(i, role = %m.role, content_bytes, "coalesce: message sizes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(capabilities: ModelCapabilities) -> ModelContext {
        ModelContext {
            capabilities,
            ..ModelContext::passthrough()
        }
    }

    // ── Stage 1 ───────────────────────────────────────────────────────────

    /// The full scrub-rule matrix lives in `crate::normalize::history`; this
    /// covers only the wiring.
    #[test]
    fn reasoning_is_stripped_and_reported() {
        let mut body = json!({
            "messages": [
                {"role": "assistant", "content": "hello", "reasoning_content": "ramble"},
            ]
        });
        assert!(shape_messages(&mut body, &ctx(ModelCapabilities::empty())));
        assert!(body["messages"][0].get("reasoning_content").is_none());
    }

    #[test]
    fn no_messages_array_reports_no_change() {
        let mut body = json!({"model": "m"});
        assert!(!shape_messages(&mut body, &ctx(ModelCapabilities::empty())));
        assert_eq!(body, json!({"model": "m"}));
    }

    #[test]
    fn clean_history_reports_no_change() {
        let mut body = json!({"messages": [{"role": "user", "content": "hi"}]});
        let before = body.clone();
        assert!(!shape_messages(&mut body, &ctx(ModelCapabilities::empty())));
        assert_eq!(body, before);
    }

    // ── Stage 2 fast paths ────────────────────────────────────────────────
    // Each must leave the body alone: the proxy relies on a `false` return to
    // forward the client's original bytes.

    #[test]
    fn unknown_capabilities_skip_coalescing() {
        let mut body = json!({"messages": [
            {"role": "user", "content": "one"},
            {"role": "user", "content": "two"},
        ]});
        let before = body.clone();
        assert!(!shape_messages(&mut body, &ctx(ModelCapabilities::empty())));
        assert_eq!(body, before, "consecutive user messages must survive");
    }

    #[test]
    fn system_role_without_strict_turns_skips_coalescing() {
        let mut body = json!({"messages": [
            {"role": "user", "content": "one"},
            {"role": "user", "content": "two"},
        ]});
        let before = body.clone();
        assert!(!shape_messages(
            &mut body,
            &ctx(ModelCapabilities::SUPPORTS_SYSTEM_ROLE)
        ));
        assert_eq!(body, before);
    }

    #[test]
    fn undeserialisable_messages_are_left_alone() {
        // `role` must be a string; a number makes the whole array unreadable.
        let mut body = json!({"messages": [{"role": 7, "content": "x"}]});
        let before = body.clone();
        assert!(!shape_messages(
            &mut body,
            &ctx(ModelCapabilities::REQUIRES_STRICT_TURNS)
        ));
        assert_eq!(body, before);
    }

    // ── Stage 2 proper ────────────────────────────────────────────────────

    #[test]
    fn strict_turns_merges_consecutive_same_role_messages() {
        let mut body = json!({"messages": [
            {"role": "user", "content": "one"},
            {"role": "user", "content": "two"},
        ]});
        assert!(shape_messages(
            &mut body,
            &ctx(ModelCapabilities::REQUIRES_STRICT_TURNS)
        ));
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["content"], "one\n\ntwo");
    }

    #[test]
    fn coalescing_preserves_tool_call_ids() {
        let mut body = json!({"messages": [
            {"role": "user", "content": "go"},
            {"role": "tool", "tool_call_id": "call_1", "content": "result"},
        ]});
        assert!(shape_messages(
            &mut body,
            &ctx(ModelCapabilities::REQUIRES_STRICT_TURNS)
        ));
        assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
    }

    /// Both stages on one body: the strip must happen before the merge, or the
    /// `<think>` block would be buried inside merged content.
    #[test]
    fn both_stages_apply_to_the_same_body() {
        let mut body = json!({"messages": [
            {"role": "assistant", "content": "<think>hidden</think>a"},
            {"role": "assistant", "content": "b"},
        ]});
        assert!(shape_messages(
            &mut body,
            &ctx(ModelCapabilities::REQUIRES_STRICT_TURNS)
        ));
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["content"], "a\n\nb");
    }

    /// Top-level fields are stage 4's business; stage 2 must not touch them.
    #[test]
    fn non_message_fields_are_untouched() {
        let mut body = json!({
            "model": "m",
            "anything_at_all": {"deep": [1, 2]},
            "messages": [
                {"role": "user", "content": "one"},
                {"role": "user", "content": "two"},
            ],
        });
        shape_messages(&mut body, &ctx(ModelCapabilities::REQUIRES_STRICT_TURNS));
        assert_eq!(body["model"], "m");
        assert_eq!(body["anything_at_all"], json!({"deep": [1, 2]}));
    }
}
