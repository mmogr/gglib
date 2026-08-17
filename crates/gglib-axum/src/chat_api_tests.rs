//! Tests for [`super`] — the chat proxy's request shaping.

use super::*;
use gglib_core::domain::ModelCapabilities;

#[test]
fn test_tools_stripped_when_not_supported() {
    let tools = Some(vec![
        serde_json::json!({"type": "function", "function": {"name": "get_weather"}}),
    ]);
    let tool_choice = Some(serde_json::json!("auto"));
    let mut body = serde_json::json!({});
    apply_tools_to_body(&mut body, &tools, &tool_choice, ModelCapabilities::empty());

    assert!(body.get("tools").is_none(), "tools should be stripped");
    assert!(
        body.get("tool_choice").is_none(),
        "tool_choice should be stripped"
    );
}

#[test]
fn test_tools_forwarded_when_supported() {
    let tool = serde_json::json!({"type": "function", "function": {"name": "get_weather"}});
    let tools = Some(vec![tool.clone()]);
    let tool_choice = Some(serde_json::json!("auto"));
    let mut body = serde_json::json!({});
    apply_tools_to_body(
        &mut body,
        &tools,
        &tool_choice,
        ModelCapabilities::SUPPORTS_TOOL_CALLS,
    );

    let tools_in_body = body.get("tools").expect("tools should be present");
    assert_eq!(
        tools_in_body,
        &serde_json::json!([tool]),
        "tools should match"
    );

    let tc_in_body = body
        .get("tool_choice")
        .expect("tool_choice should be present");
    assert_eq!(
        tc_in_body,
        &serde_json::json!("auto"),
        "tool_choice should match"
    );
}

#[test]
fn test_no_op_when_no_tools_sent() {
    // tools: None, tool_choice: None — body should stay empty regardless of capability
    let mut body_no_cap = serde_json::json!({});
    apply_tools_to_body(&mut body_no_cap, &None, &None, ModelCapabilities::empty());

    let mut body_with_cap = serde_json::json!({});
    apply_tools_to_body(
        &mut body_with_cap,
        &None,
        &None,
        ModelCapabilities::SUPPORTS_TOOL_CALLS,
    );

    assert!(body_no_cap.get("tools").is_none());
    assert!(body_no_cap.get("tool_choice").is_none());
    assert!(body_with_cap.get("tools").is_none());
    assert!(body_with_cap.get("tool_choice").is_none());
}

/// JSON-boundary tests for `UpdateConversationRequest.system_prompt`,
/// mirroring the coverage added for `UpdateModelRequest.server_defaults`
/// and `UpdateSettingsRequest`. Deserializes raw JSON to prove
/// `serde_with::rust::double_option` distinguishes an omitted key from
/// an explicit `null` — without it, `PUT /api/conversations/:id` with
/// `{"system_prompt": null}` (the frontend's "clear system prompt"
/// request) silently no-ops instead of clearing the prompt.
#[test]
fn update_conversation_request_omitted_system_prompt_is_none() {
    let req: UpdateConversationRequest = serde_json::from_str("{}").unwrap();
    assert_eq!(req.system_prompt, None, "omitted key must be None");
}

#[test]
fn update_conversation_request_explicit_null_is_some_none() {
    let req: UpdateConversationRequest =
        serde_json::from_str(r#"{"system_prompt": null}"#).unwrap();
    assert_eq!(
        req.system_prompt,
        Some(None),
        "explicit null must clear the system prompt (Some(None))"
    );
}

#[test]
fn update_conversation_request_populated_value_is_some_some() {
    let req: UpdateConversationRequest =
        serde_json::from_str(r#"{"system_prompt": "You are a pirate."}"#).unwrap();
    assert_eq!(
        req.system_prompt,
        Some(Some("You are a pirate.".to_string()))
    );
}

// =========================================================================
// The reasoning controls on the chat proxy request
// =========================================================================

use gglib_core::domain::ReasoningEffort;

/// The camelCase spelling the GUI sends, and the one the rest of this DTO
/// already uses (`maxTokens`, `topP`).
#[test]
fn both_reasoning_controls_deserialize_from_the_camel_case_wire_form() {
    let req: ChatProxyRequest = serde_json::from_str(
        r#"{"port":9000,"messages":[],"reasoningEffort":"xhigh","reasoningBudgetTokens":8192}"#,
    )
    .expect("parses");

    assert_eq!(req.reasoning_effort, Some(ReasoningEffort::XHigh));
    assert_eq!(req.reasoning_budget_tokens, Some(8192));
}

/// Omitting them must leave the layers beneath to resolve, exactly as omitting
/// `temperature` does. A `Some(default)` here would silently outrank the
/// model's own recipe on every request the GUI sends.
#[test]
fn omitting_them_leaves_the_request_layer_silent() {
    let req: ChatProxyRequest =
        serde_json::from_str(r#"{"port":9000,"messages":[]}"#).expect("parses");

    assert_eq!(req.reasoning_effort, None);
    assert_eq!(req.reasoning_budget_tokens, None);
}

/// `-1` is upstream's defer sentinel and `0` means stop thinking. Both are
/// meaningful values a client may send, and neither may be mistaken for an
/// absence.
#[test]
fn the_budget_sentinels_are_values_not_absences() {
    for (sent, expected) in [("-1", -1), ("0", 0)] {
        let body = format!(r#"{{"port":9000,"messages":[],"reasoningBudgetTokens":{sent}}}"#);
        let req: ChatProxyRequest = serde_json::from_str(&body).expect("parses");
        assert_eq!(req.reasoning_budget_tokens, Some(expected));
    }
}

/// **There is no `none` level anywhere in gglib.** Erasing the kwarg yields the
/// template's own default, which is what omitting the field already does — so
/// accepting `"none"` here would give one act two spellings, one of which does
/// something subtly different.
#[test]
fn none_is_not_a_level() {
    let err = serde_json::from_str::<ChatProxyRequest>(
        r#"{"port":9000,"messages":[],"reasoningEffort":"none"}"#,
    )
    .expect_err("'none' must be refused");
    assert!(err.to_string().contains("none"), "{err}");
}

/// An unreadable level is refused rather than forwarded. Upstream validates
/// this field not at all — `"banana"` is accepted and rendered into the prompt
/// verbatim — so gglib's "no" is the only "no" there is.
#[test]
fn an_unknown_level_is_refused_rather_than_passed_through() {
    assert!(
        serde_json::from_str::<ChatProxyRequest>(
            r#"{"port":9000,"messages":[],"reasoningEffort":"banana"}"#,
        )
        .is_err()
    );
}
