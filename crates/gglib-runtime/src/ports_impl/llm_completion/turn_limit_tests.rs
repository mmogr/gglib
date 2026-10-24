//! A loop turn's limit, as the adapter puts it on the wire: given when the
//! ladder named no `max_tokens`, more for a `reasoning` model, room above a
//! thinking budget, never in place of a number something named, and never
//! on a send to another machine.

use serde_json::{Value, json};

use super::super::images::ImageUrls;
use super::super::{FarMachine, LlmCompletionAdapter};
use super::{limit_turn, reasoning_turn_tokens};
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::{DefaultsOrigin, InferenceConfig};
use gglib_core::request_pipeline::ModelContext;

/// The body a loop's adapter sends for one message, with `sampling` as
/// the person's choice.
fn sent(sampling: Option<InferenceConfig>, limit: bool) -> Value {
    sent_by(
        LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned())),
        sampling,
        limit,
    )
}

/// The body `adapter` sends for one message, with `sampling` as the
/// person's choice.
fn sent_by(adapter: LlmCompletionAdapter, sampling: Option<InferenceConfig>, limit: bool) -> Value {
    adapter
        .with_sampling(sampling)
        .with_turn_limit(limit)
        .shaped_body(
            &[AgentMessage::user("draw a T rex")],
            &[],
            &ImageUrls::default(),
            false,
        )
        .unwrap()
}

fn thinking(budget: i32) -> InferenceConfig {
    InferenceConfig {
        reasoning_budget_tokens: Some(budget),
        ..InferenceConfig::default()
    }
}

#[test]
fn a_turn_that_names_no_limit_is_given_16384_tokens() {
    assert_eq!(sent(None, true)["max_tokens"], json!(16_384));
}

#[test]
fn an_adapter_without_the_limit_sends_none() {
    assert!(sent(None, false).get("max_tokens").is_none());
}

#[test]
fn a_limit_someone_named_is_kept_above_the_default_and_below_it() {
    for named in [512, 65_536] {
        let sampling = Some(InferenceConfig {
            max_tokens: Some(named),
            ..InferenceConfig::default()
        });
        assert_eq!(sent(sampling, true)["max_tokens"], json!(named));
    }
}

/// The `high` rung's 16384 thinking tokens still leave 8192 for the answer;
/// a small budget, no budget (`-1`) and no thinking (`0`) leave the default.
#[test]
fn a_thinking_budget_gets_room_for_the_answer_past_it() {
    for (budget, limit) in [
        (16_384, 24_576),
        (32_768, 40_960),
        (4096, 16_384),
        (-1, 16_384),
        (0, 16_384),
    ] {
        assert_eq!(
            sent(Some(thinking(budget)), true)["max_tokens"],
            json!(limit),
            "budget {budget}"
        );
    }
}

/// A turn for a paired machine's model carries no limit from this side:
/// that machine's own `max_tokens`, or its recipe's, applies there.
#[test]
fn a_turn_for_another_machine_is_not_limited() {
    let far = FarMachine {
        key: "far-key".to_owned(),
        name: "desk".to_owned(),
    };
    let adapter = LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
        .with_far_machine(Some(far));
    let body = sent_by(adapter, Some(thinking(16_384)), true);
    assert!(body.get("max_tokens").is_none(), "{body}");
}

/// A `reasoning` model whose own recipe names no `max_tokens`, so the
/// ladder leaves none (a published recipe stands in the `reasoning` one's
/// place).
fn reasoning_with_a_recipe_of_its_own() -> ModelContext {
    ModelContext {
        tags: vec!["reasoning".to_owned()],
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.6),
            ..InferenceConfig::default()
        }),
        defaults_origin: Some(DefaultsOrigin::Published),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

/// A `reasoning` model gets the `reasoning` recipe's 32768 where nothing
/// names a limit, and still room past a larger thinking budget.
#[test]
fn a_reasoning_model_that_names_no_limit_is_given_32768_tokens() {
    assert_eq!(reasoning_turn_tokens(), 32_768);
    for (sampling, limit) in [
        (None, 32_768),
        (Some(thinking(-1)), 32_768),
        (Some(thinking(16_384)), 32_768),
        (Some(thinking(32_768)), 40_960),
    ] {
        let adapter = LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
            .with_model_context(reasoning_with_a_recipe_of_its_own());
        assert_eq!(
            sent_by(adapter, sampling.clone(), true)["max_tokens"],
            json!(limit),
            "{sampling:?}"
        );
    }
}

/// The rule itself: the base is the model's class, and a named number is
/// never touched.
#[test]
fn limit_turn_takes_its_base_from_the_class_and_keeps_a_named_number() {
    let limited = |reasoning: bool, mut body: Value| {
        limit_turn(&mut body, reasoning);
        body["max_tokens"].clone()
    };
    assert_eq!(limited(false, json!({})), json!(16_384));
    assert_eq!(limited(true, json!({})), json!(32_768));
    assert_eq!(
        limited(true, json!({ "reasoning_budget_tokens": 40_000 })),
        json!(48_192)
    );
    assert_eq!(limited(true, json!({ "max_tokens": 512 })), json!(512));
}
