//! OpenAI request-body construction for [`LlmCompletionAdapter`].
//!
//! Pure translation from domain types to the OpenAI-compatible JSON wire
//! format, kept separate from the transport concerns in
//! [`super`] so the resulting body can be asserted on directly in tests
//! without an HTTP round trip.
//!
//! **Translation only.** Every transform that *reshapes* a request — the
//! reasoning strip, capability coalescing, resolving the sampling hierarchy —
//! belongs to [`gglib_core::request_pipeline`] and runs on the finished body in
//! [`super::LlmCompletionAdapter::chat_stream`], so the agent path and the proxy
//! apply the same ones in the same order. What this module writes is only what
//! the *caller* asked for.
//!
//! [`LlmCompletionAdapter`]: super::LlmCompletionAdapter

use serde_json::{Value, json};

use gglib_core::{
    domain::InferenceConfig,
    domain::agent::{AgentMessage, ToolCall, ToolDefinition},
    ports::ResponseFormat,
};

// =============================================================================
// Wire-format helpers
// =============================================================================

/// Map a domain [`AgentMessage`] to the OpenAI `messages` array element.
fn message_to_openai(msg: &AgentMessage) -> Value {
    match msg {
        AgentMessage::System { content } => {
            json!({ "role": "system", "content": content })
        }
        AgentMessage::User { content } => {
            json!({ "role": "user", "content": content })
        }
        AgentMessage::Assistant { content } => {
            // When tool_calls are present but text is None, omit the
            // "content" field entirely rather than sending `"content": null`.
            // Some LLM backends do not handle an explicit null well when
            // tool_calls is populated.  When there are no tool_calls and
            // text is None, we still send null to signal an empty reply.
            let has_tool_calls = !content.tool_calls.is_empty();
            let mut obj = if content.text.is_none() && has_tool_calls {
                json!({ "role": "assistant" })
            } else {
                json!({
                    "role": "assistant",
                    "content": content.text.as_deref().map_or(Value::Null, |s| Value::String(s.to_owned())),
                })
            };
            if has_tool_calls {
                let calls: Vec<Value> =
                    content.tool_calls.iter().map(tool_call_to_openai).collect();
                obj["tool_calls"] = Value::Array(calls);
            }
            obj
        }
        AgentMessage::Tool {
            tool_call_id,
            content,
        } => {
            json!({ "role": "tool", "tool_call_id": tool_call_id, "content": content })
        }
    }
}

/// Map a domain [`ToolCall`] to the OpenAI `tool_calls` array element.
///
/// The OpenAI API requires `arguments` to be a **JSON string**, not an object.
fn tool_call_to_openai(tc: &ToolCall) -> Value {
    json!({
        "id": tc.id,
        "type": "function",
        "function": {
            "name": tc.name,
            // arguments must be a JSON *string* per OpenAI spec
            "arguments": tc.arguments.to_string(),
        },
    })
}

/// Map a domain [`ToolDefinition`] to the OpenAI `tools` array element.
fn tool_def_to_openai(def: &ToolDefinition) -> Value {
    let parameters = def
        .input_schema
        .clone()
        .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));

    json!({
        "type": "function",
        "function": {
            "name": def.name,
            "description": def.description,
            "parameters": parameters,
        },
    })
}

// =============================================================================
// Body construction
// =============================================================================

/// Build the full OpenAI `/v1/chat/completions` request body.
///
/// Always emits `model`, `messages`, `stream` and `return_progress`.  `tools` /
/// `tool_choice` are added only when `tools` is non-empty, and sampling keys
/// only for the `Some` fields of `sampling`.
pub(super) fn build_chat_body(
    model: &str,
    messages: &[AgentMessage],
    tools: &[ToolDefinition],
    sampling: Option<&InferenceConfig>,
    response_format: Option<&ResponseFormat>,
) -> Value {
    let openai_messages: Vec<Value> = messages.iter().map(message_to_openai).collect();
    let openai_tools: Vec<Value> = tools.iter().map(tool_def_to_openai).collect();

    let mut body = json!({
        "model": model,
        "messages": openai_messages,
        "stream": true,
        "return_progress": true,
    });
    if !openai_tools.is_empty() {
        body["tools"] = json!(openai_tools);
        body["tool_choice"] = json!("auto");
    }
    // Write the caller's own sampling parameters — the top layer of the
    // hierarchy, in the same place an external client would put them, so the
    // shared pipeline can read them back and resolve the rest beneath them.
    //
    // Applied through the same serde-driven helper the proxy uses, so the two
    // request paths cannot drift.  A hand-rolled field-by-field copy here is
    // what silently dropped `presence_penalty` and `min_p` when they were added
    // to InferenceConfig.  `to_openai_json_patch` emits only `Some` fields,
    // already snake_cased.
    if let Some(s) = sampling
        && let Some(obj) = body.as_object_mut()
    {
        for (k, v) in s.to_openai_json_patch() {
            obj.insert(k, v);
        }
    }

    // Inject structured-output constraints when requested.
    if let Some(fmt) = response_format {
        match fmt {
            ResponseFormat::JsonSchema { schema, strict } => {
                body["response_format"] = json!({
                    "type": "json_schema",
                    "json_schema": { "schema": schema, "strict": strict }
                });
            }
            ResponseFormat::Grammar { gbnf } => {
                body["grammar"] = json!(gbnf);
            }
        }
    }

    body
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every value is exactly representable in both `f32` and `f64`, so the
    /// `f32 → f64` widening `serde_json` performs is lossless and the values
    /// can be compared against `json!` literals directly.
    fn full_config() -> InferenceConfig {
        InferenceConfig {
            temperature: Some(0.5),
            top_p: Some(0.75),
            top_k: Some(20),
            max_tokens: Some(512),
            repeat_penalty: Some(1.25),
            presence_penalty: Some(1.5),
            min_p: Some(0.0625),
        }
    }

    fn messages() -> Vec<AgentMessage> {
        vec![AgentMessage::User {
            content: "hello".to_string(),
        }]
    }

    /// Regression guard for #611.
    ///
    /// `presence_penalty` and `min_p` were added to [`InferenceConfig`] after
    /// this adapter was written, and the hand-rolled serialization that used to
    /// live here silently dropped them while the proxy sent them.  Sampling is
    /// now applied via [`InferenceConfig::to_openai_json_patch`] — the proxy's
    /// own helper — so the two cannot diverge again.
    #[test]
    fn sampling_emits_all_seven_openai_keys() {
        let config = full_config();
        let body = build_chat_body("m", &messages(), &[], Some(&config), None);

        assert_eq!(body["temperature"], json!(0.5));
        assert_eq!(body["top_p"], json!(0.75));
        assert_eq!(body["top_k"], json!(20));
        assert_eq!(body["max_tokens"], json!(512));
        assert_eq!(body["repeat_penalty"], json!(1.25));
        assert_eq!(body["presence_penalty"], json!(1.5));
        assert_eq!(body["min_p"], json!(0.0625));
    }

    #[test]
    fn sampling_none_fields_are_omitted() {
        let config = InferenceConfig {
            temperature: Some(0.5),
            presence_penalty: Some(1.5),
            ..Default::default()
        };
        let body = build_chat_body("m", &messages(), &[], Some(&config), None);
        let obj = body.as_object().expect("body is an object");

        assert!(obj.contains_key("temperature"));
        assert!(obj.contains_key("presence_penalty"));
        for key in ["top_p", "top_k", "max_tokens", "repeat_penalty", "min_p"] {
            assert!(!obj.contains_key(key), "{key} should be absent, not null");
        }
        // Unset fields must be omitted entirely rather than sent as `null`.
        assert!(obj.values().all(|v| !v.is_null()));
    }

    #[test]
    fn sampling_absent_emits_no_sampling_keys() {
        let body = build_chat_body("m", &messages(), &[], None, None);
        let obj = body.as_object().expect("body is an object");

        for key in [
            "temperature",
            "top_p",
            "top_k",
            "max_tokens",
            "repeat_penalty",
            "presence_penalty",
            "min_p",
        ] {
            assert!(!obj.contains_key(key));
        }
    }

    #[test]
    fn sampling_does_not_clobber_transport_fields() {
        let config = full_config();
        let tools = vec![ToolDefinition::new("read_file")];
        let format = ResponseFormat::Grammar {
            gbnf: "root ::= \"ok\"".to_string(),
        };
        let body = build_chat_body(
            "my-model",
            &messages(),
            &tools,
            Some(&config),
            Some(&format),
        );

        assert_eq!(body["model"], json!("my-model"));
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["return_progress"], json!(true));
        assert_eq!(body["tool_choice"], json!("auto"));
        assert_eq!(body["tools"][0]["function"]["name"], json!("read_file"));
        assert_eq!(body["grammar"], json!("root ::= \"ok\""));
        assert_eq!(body["messages"][0]["content"], json!("hello"));
        // …and the sampling keys still landed alongside them.
        assert_eq!(body["min_p"], json!(0.0625));
    }
}
