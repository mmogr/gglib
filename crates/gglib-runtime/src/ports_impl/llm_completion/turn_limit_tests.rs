//! A loop turn's limit, as the adapter puts it on the wire: given when the
//! ladder named no `max_tokens`, more for a `reasoning` model, room above a
//! thinking budget, never in place of a number something named, and never
//! on a send to another machine.

use serde_json::{Value, json};

use super::super::images::ImageUrls;
use super::super::{Door, LlmCompletionAdapter};
use super::{limit_turn, reasoning_turn_tokens};
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::{DefaultsOrigin, InferenceConfig};
use gglib_core::request_pipeline::{ModelContext, SamplingLayers};

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

/// The default limit keeps 8192 of its tokens for the answer, so the
/// thinking is given the other 8192.
#[test]
fn a_turn_that_names_no_limit_is_given_16384_tokens() {
    let body = sent(None, true);
    assert_eq!(body["max_tokens"], json!(16_384));
    assert_eq!(body["reasoning_budget_tokens"], json!(8_192), "{body}");
}

#[test]
fn an_adapter_without_the_limit_sends_none() {
    let body = sent(None, false);
    assert!(body.get("max_tokens").is_none(), "{body}");
    assert!(body.get("reasoning_budget_tokens").is_none(), "{body}");
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
        let body = sent(Some(thinking(budget)), true);
        assert_eq!(body["max_tokens"], json!(limit), "budget {budget}");
        assert_eq!(body["reasoning_budget_tokens"], json!(budget), "{body}");
    }
}

/// A turn for a paired machine's model carries no limit and no thinking
/// budget from this side: that machine's own settings, or its recipe's,
/// apply there.
#[test]
fn a_turn_for_another_machine_is_not_limited() {
    let far = || {
        let door = Door {
            root_url: String::new(),
            key: Some("far-key".to_owned()),
            name: Some("desk".to_owned()),
        };
        LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned())).with_door(Some(door))
    };
    let body = sent_by(far(), Some(thinking(16_384)), true);
    assert!(body.get("max_tokens").is_none(), "{body}");

    let body = sent_by(far(), None, true);
    assert!(body.get("max_tokens").is_none(), "{body}");
    assert!(body.get("reasoning_budget_tokens").is_none(), "{body}");
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

/// A `reasoning` model with no recipe of its own: the bottom rung holds the
/// built-in `reasoning` recipe, whose `max_tokens` is 32768.
fn reasoning_on_the_built_in_recipe() -> ModelContext {
    ModelContext {
        tags: vec!["reasoning".to_owned()],
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

/// The body a loop's adapter sends for `model`, with `sampling` as the
/// person's choice.
fn sent_for(model: ModelContext, sampling: Option<InferenceConfig>) -> Value {
    sent_by(
        LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
            .with_model_context(model),
        sampling,
        true,
    )
}

/// The issue's first check: gglib's own limit, the one written here or the
/// built-in recipe's, gives the thinking that limit less the answer's room;
/// a limit the request names, or a budget it names, derives nothing.
#[test]
fn gglibs_own_limit_bounds_the_thinking_and_a_named_number_does_not() {
    let written = sent_for(reasoning_with_a_recipe_of_its_own(), None);
    assert_eq!(written["max_tokens"], json!(32_768), "{written}");
    assert_eq!(
        written["reasoning_budget_tokens"],
        json!(24_576),
        "{written}"
    );

    let named = Some(InferenceConfig {
        max_tokens: Some(500),
        ..InferenceConfig::default()
    });
    let named = sent_for(reasoning_with_a_recipe_of_its_own(), named);
    assert_eq!(named["max_tokens"], json!(500), "{named}");
    assert!(named.get("reasoning_budget_tokens").is_none(), "{named}");

    let recipe = sent_for(reasoning_on_the_built_in_recipe(), None);
    assert_eq!(recipe["max_tokens"], json!(32_768), "{recipe}");
    assert_eq!(recipe["reasoning_budget_tokens"], json!(24_576), "{recipe}");
    for body in [&written, &recipe] {
        assert!(body.get("reasoning_budget_message").is_none(), "{body}");
    }

    let off = sent_for(reasoning_on_the_built_in_recipe(), Some(thinking(0)));
    assert_eq!(off["reasoning_budget_tokens"], json!(0), "{off}");
}

/// A `reasoning` model on the built-in recipe, with `stored` on its row
/// from `origin`.
fn reasoning_storing(stored: InferenceConfig, origin: DefaultsOrigin) -> ModelContext {
    ModelContext {
        inference_defaults: Some(stored),
        defaults_origin: Some(origin),
        ..reasoning_on_the_built_in_recipe()
    }
}

fn limit_of(max_tokens: u32) -> InferenceConfig {
    InferenceConfig {
        max_tokens: Some(max_tokens),
        ..InferenceConfig::default()
    }
}

/// The body a loop's adapter sends for `model` under `layers`, with no
/// choice of the person's.
fn sent_under(model: ModelContext, layers: SamplingLayers) -> Value {
    sent_by(
        LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
            .with_model_context(model)
            .with_layers(layers),
        None,
        true,
    )
}

/// The recipe's own number, 32768, named by any other rung derives nothing:
/// what decides is which rung named it, never the number.
#[test]
fn the_recipes_number_named_by_another_rung_derives_nothing() {
    let named = limit_of(32_768);
    let cases = [
        (
            "the person",
            sent_for(reasoning_on_the_built_in_recipe(), Some(named.clone())),
        ),
        (
            "the command line",
            sent_under(
                reasoning_on_the_built_in_recipe(),
                SamplingLayers {
                    cli_override: Some(named.clone()),
                    ..SamplingLayers::default()
                },
            ),
        ),
        (
            "a profile",
            sent_under(
                reasoning_on_the_built_in_recipe(),
                SamplingLayers {
                    profile: Some(named.clone()),
                    ..SamplingLayers::default()
                },
            ),
        ),
        (
            "the global settings",
            sent_under(
                reasoning_on_the_built_in_recipe(),
                SamplingLayers {
                    global: Some(named.clone()),
                    ..SamplingLayers::default()
                },
            ),
        ),
        (
            "the model's own settings",
            sent_for(reasoning_storing(named.clone(), DefaultsOrigin::User), None),
        ),
        (
            "a published recipe",
            sent_for(
                reasoning_storing(named.clone(), DefaultsOrigin::Published),
                None,
            ),
        ),
        (
            "a measured recipe",
            sent_for(reasoning_storing(named, DefaultsOrigin::Measured), None),
        ),
    ];
    for (who, body) in cases {
        assert_eq!(body["max_tokens"], json!(32_768), "{who}: {body}");
        assert!(
            body.get("reasoning_budget_tokens").is_none(),
            "{who}: {body}"
        );
    }
}

/// A small limit from outside is never taken for a budget, which would be
/// `0`, thinking off, or a budget nobody asked for.
#[test]
fn a_small_limit_from_outside_never_becomes_a_budget() {
    for limit in [512, 8_192, 9_000] {
        let body = sent_under(
            ModelContext::passthrough(),
            SamplingLayers {
                global: Some(limit_of(limit)),
                ..SamplingLayers::default()
            },
        );
        assert_eq!(body["max_tokens"], json!(limit), "{body}");
        assert!(body.get("reasoning_budget_tokens").is_none(), "{body}");
    }
}

/// A person's own settings for the model that name no limit leave the
/// built-in recipe's beneath them, and the budget it derives.
#[test]
fn the_models_own_settings_without_a_limit_keep_the_recipes_budget() {
    let own = InferenceConfig {
        temperature: Some(0.7),
        ..InferenceConfig::default()
    };
    let body = sent_for(reasoning_storing(own, DefaultsOrigin::User), None);
    assert_eq!(body["max_tokens"], json!(32_768), "{body}");
    assert_eq!(body["reasoning_budget_tokens"], json!(24_576), "{body}");
}

/// A budget is never touched, whoever named it and whatever it says: `0`
/// is thinking off, `-1` the launch default, and `-2` is llama-server's to
/// refuse.
#[test]
fn a_named_budget_is_kept_whoever_named_it() {
    for budget in [0, -1, 4_096] {
        let cases = [
            (
                "the person",
                sent_for(reasoning_on_the_built_in_recipe(), Some(thinking(budget))),
            ),
            (
                "a profile",
                sent_under(
                    reasoning_on_the_built_in_recipe(),
                    SamplingLayers {
                        profile: Some(thinking(budget)),
                        ..SamplingLayers::default()
                    },
                ),
            ),
            (
                "the global settings",
                sent_under(
                    ModelContext::passthrough(),
                    SamplingLayers {
                        global: Some(thinking(budget)),
                        ..SamplingLayers::default()
                    },
                ),
            ),
            (
                "the model's own settings",
                sent_for(
                    reasoning_storing(thinking(budget), DefaultsOrigin::User),
                    None,
                ),
            ),
        ];
        for (who, body) in cases {
            assert_eq!(
                body["reasoning_budget_tokens"],
                json!(budget),
                "{who}: {body}"
            );
        }
    }
    let refused = sent_for(reasoning_on_the_built_in_recipe(), Some(thinking(-2)));
    assert_eq!(refused["reasoning_budget_tokens"], json!(-2), "{refused}");
}
