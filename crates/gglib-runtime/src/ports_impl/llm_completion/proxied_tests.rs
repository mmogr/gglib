//! A send through a door is proxied: the endpoint shapes it as it shapes any
//! client's, so the adapter sends what a client sends. It folds no sampling
//! ladder, merges no turns and trims nothing; it keeps the person's own
//! choice, the first call's demand and, for this machine's endpoint, the
//! turn's limit.

use serde_json::{Value, json};

use super::images::ImageUrls;
use super::*;
use gglib_core::domain::agent::{AssistantContent, ToolDefinition};
use gglib_core::domain::{DefaultsOrigin, ModelCapabilities};

/// This machine's endpoint, as `ProxyOps::door` opens it.
fn endpoint() -> Door {
    Door {
        root_url: "http://127.0.0.1:0".to_owned(),
        key: Some("this-machines-key".to_owned()),
        name: None,
    }
}

/// A model whose catalogue row would shape every stage: strict turns, its
/// own temperature, and a context of 1024 tokens.
fn shaped_model() -> ModelContext {
    ModelContext {
        capabilities: ModelCapabilities::REQUIRES_STRICT_TURNS,
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.3),
            ..InferenceConfig::default()
        }),
        context_length: Some(1_024),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

/// The adapter a turn on this machine's model is composed with, through
/// `door`.
fn through(door: Option<Door>, sampling: Option<InferenceConfig>) -> LlmCompletionAdapter {
    LlmCompletionAdapter::new("http://127.0.0.1:0", Some("7".to_owned()))
        .with_door(door)
        .with_model_context(shaped_model())
        .with_sampling(sampling)
        .with_turn_limit(true)
}

/// Two user messages in a row and a long tool result: what strict turns
/// merge and a 1024-token budget trims.
fn conversation() -> Vec<AgentMessage> {
    vec![
        AgentMessage::Tool {
            tool_call_id: "call_1".to_owned(),
            content: "x".repeat(30_000),
            images: Vec::new(),
        },
        AgentMessage::user("first"),
        AgentMessage::user("second"),
    ]
}

fn body(adapter: &LlmCompletionAdapter, messages: &[AgentMessage], must_call: bool) -> Value {
    let tool = ToolDefinition::new("builtin:generate_image");
    adapter
        .shaped_body(messages, &[tool], &ImageUrls::default(), must_call)
        .unwrap()
}

/// The endpoint shapes it: the model's own temperature is not written, the
/// two user messages stay two, and the long tool result goes whole.
#[test]
fn a_send_through_this_machines_endpoint_is_not_shaped_here() {
    let sent = body(&through(Some(endpoint()), None), &conversation(), false);

    assert!(sent.get("temperature").is_none(), "{sent}");
    let messages = sent["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3, "no turn merged");
    assert_eq!(messages[0]["content"].as_str().unwrap().len(), 30_000);
    assert_eq!(sent["model"], "7");
}

/// The same model on llama-server's port is shaped, so the test above is
/// about the door and not about a context that shapes nothing: its own
/// temperature is written, the two user messages are merged, and the long
/// tool result is over its 1024-token budget.
#[test]
fn the_same_send_to_llama_server_is_shaped() {
    let adapter = through(None, None);
    let short = [AgentMessage::user("first"), AgentMessage::user("second")];
    let sent = body(&adapter, &short, false);

    assert_eq!(sent["temperature"], json!(0.3_f32));
    assert_eq!(sent["messages"].as_array().unwrap().len(), 1, "{sent}");
    let tool = ToolDefinition::new("builtin:generate_image");
    let over = adapter.shaped_body(&conversation(), &[tool], &ImageUrls::default(), false);
    assert!(over.is_err(), "the budget was not applied");
}

/// What a person chose still goes: thinking off is `reasoning_budget_tokens:
/// 0`, which the endpoint honours from any client.
#[test]
fn the_persons_own_choice_reaches_the_endpoint() {
    let off = InferenceConfig {
        reasoning_budget_tokens: Some(0),
        ..InferenceConfig::default()
    };
    let sent = body(
        &through(Some(endpoint()), Some(off)),
        &conversation(),
        false,
    );

    assert_eq!(sent["reasoning_budget_tokens"], json!(0));
}

/// Draw's first reply must call the tool: `required` reaches the endpoint as
/// it was written, for the endpoint's pipeline to read as a client's.
#[test]
fn the_first_calls_demand_reaches_the_endpoint() {
    let sent = body(&through(Some(endpoint()), None), &conversation(), true);

    assert_eq!(sent["tool_choice"], "required");
    assert!(sent.get("grammar").is_none(), "{sent}");
}

/// This machine's endpoint takes the turn's limit as the client's own
/// budget; a paired machine's applies its own settings.
#[test]
fn only_this_machines_endpoint_is_given_the_turns_limit() {
    let local = body(&through(Some(endpoint()), None), &conversation(), false);
    assert_eq!(local["max_tokens"], json!(16_384));

    let far = Door {
        name: Some("desk".to_owned()),
        ..endpoint()
    };
    let far = body(&through(Some(far), None), &conversation(), false);
    assert!(far.get("max_tokens").is_none(), "{far}");
}

/// A `max_tokens` the endpoint's ladder will name, from this machine's
/// settings or the model's own values, is left for the endpoint to send: a
/// limit from here would outrank it, since the endpoint honours a client's
/// budget whatever it trusts.
#[test]
fn a_limit_the_ladder_names_is_left_for_the_endpoint() {
    let global = SamplingLayers {
        global: Some(InferenceConfig {
            max_tokens: Some(65_536),
            ..InferenceConfig::default()
        }),
        ..SamplingLayers::default()
    };
    let by_settings = through(Some(endpoint()), None).with_layers(global);
    let sent = body(&by_settings, &[AgentMessage::user("hi")], false);
    assert!(sent.get("max_tokens").is_none(), "{sent}");

    let own = ModelContext {
        inference_defaults: Some(InferenceConfig {
            max_tokens: Some(512),
            ..InferenceConfig::default()
        }),
        ..shaped_model()
    };
    let by_model = through(Some(endpoint()), None).with_model_context(own);
    let sent = body(&by_model, &[AgentMessage::user("hi")], false);
    assert!(sent.get("max_tokens").is_none(), "{sent}");
}

/// The limit through this machine's endpoint is the class's: a `reasoning`
/// model whose own recipe names no `max_tokens` is given 32768. One whose
/// recipe is the `reasoning` one has that recipe's 32768 from the
/// endpoint's ladder, and none from here.
#[test]
fn a_reasoning_model_through_the_endpoint_is_given_its_classs_limit() {
    let published = through(Some(endpoint()), None).with_model_context(ModelContext {
        tags: vec!["reasoning".to_owned()],
        defaults_origin: Some(DefaultsOrigin::Published),
        ..shaped_model()
    });
    let sent = body(&published, &[AgentMessage::user("hi")], false);
    assert_eq!(sent["max_tokens"], json!(32_768));

    let recipe = through(Some(endpoint()), None).with_model_context(ModelContext {
        tags: vec!["reasoning".to_owned()],
        inference_defaults: None,
        ..shaped_model()
    });
    let sent = body(&recipe, &[AgentMessage::user("hi")], false);
    assert!(sent.get("max_tokens").is_none(), "{sent}");
}

/// A prior turn's thinking is the endpoint's to strip, as it strips any
/// client's.
#[test]
fn a_prior_turns_thinking_is_left_for_the_endpoint() {
    let messages = [
        AgentMessage::user("hi"),
        AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("<think>ramble</think>answer".to_owned()),
                tool_calls: vec![],
            },
        },
        AgentMessage::user("and?"),
    ];
    let sent = body(&through(Some(endpoint()), None), &messages, false);

    assert_eq!(
        sent["messages"][1]["content"],
        "<think>ramble</think>answer"
    );
}
