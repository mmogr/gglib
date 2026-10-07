//! Tests for [`super`]: what the conversation routes read from a request body.

use super::*;

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
