//! What a daemon turn samples with: the page's run and a paired device's
//! turn. The served model's own values and this machine's global defaults
//! are folded beneath what the request names, by the rules `gglib chat` and
//! the proxy resolve by, so a turn that names nothing sends what
//! `gglib model explain` reports.

use gglib_core::domain::{DefaultsOrigin, InferenceConfig};
use serde_json::json;

use crate::handlers::agent::run_fixture::state;
use crate::handlers::agent::turn_fixture::{
    agentic_sampling, carries_tools, doors, global, model, reasoning, sampling_of, sent, stored,
    temperature,
};

#[tokio::test]
async fn a_global_temperature_is_sent_by_the_page_and_by_a_paired_device() {
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;
    global(&state, temperature(0.42)).await;

    for (door, chat) in doors(&state, id, false).await {
        let body = sent(&state, id, chat).await;
        assert_eq!(body["temperature"], json!(0.42_f32), "{door}");
    }
}

/// A value a person set on the model outranks the global one, and a field
/// the model leaves unset still comes from the global layer: the order
/// `gglib model explain` reports.
#[tokio::test]
async fn a_value_set_on_the_model_outranks_the_global_one_as_model_explain_reports() {
    let (_dir, state) = state().await;
    let id = model(&state, stored(temperature(0.55), DefaultsOrigin::User)).await;
    let defaults = InferenceConfig {
        top_k: Some(33),
        ..temperature(0.42)
    };
    global(&state, defaults).await;
    let explained = state.models.explain_sampling(id, None).await.unwrap();
    assert_eq!(explained.resolved.temperature, Some(0.55));
    assert_eq!(explained.resolved.top_k, Some(33));

    for (door, chat) in doors(&state, id, false).await {
        let body = sent(&state, id, chat).await;
        assert_eq!(body["temperature"], json!(0.55_f32), "{door}");
        assert_eq!(body["top_k"], json!(33), "{door}");
    }
}

/// A reasoning-tagged model is sent the recipe its import wrote, whole, on a
/// turn with tools and on one without: its class has no ceiling.
#[tokio::test]
async fn a_reasoning_models_recipe_is_sent_whole_with_tools_and_without() {
    let (_dir, state) = state().await;
    let id = model(&state, reasoning).await;

    for tools in [false, true] {
        for (door, chat) in doors(&state, id, tools).await {
            let body = sent(&state, id, chat).await;
            assert_eq!(carries_tools(&body), tools, "{door}");
            let recipe = InferenceConfig::reasoning_profile();
            assert_eq!(sampling_of(&body), recipe, "{door}, tools: {tools}");
        }
    }
}

/// An ordinary model with nothing chosen is sent the floor's 0.7, and 0.3 on
/// a turn with tools: what `gglib chat` and `gglib q` send it.
#[tokio::test]
async fn an_ordinary_model_with_nothing_chosen_is_capped_on_a_turn_with_tools() {
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;

    for (tools, want) in [(false, 0.7_f32), (true, 0.3)] {
        for (door, chat) in doors(&state, id, tools).await {
            let body = sent(&state, id, chat).await;
            assert_eq!(carries_tools(&body), tools, "{door}");
            assert_eq!(body["temperature"], json!(want), "{door}");
        }
    }
}

/// Both doors follow this machine's agentic sampling switch. Stored off, a
/// turn with tools is sent the temperature it resolves to: the floor's 0.7,
/// or the 0.9 gglib guessed for the model. Stored on, or not stored, each is
/// capped.
#[tokio::test]
async fn a_turn_with_tools_is_capped_unless_agentic_sampling_is_stored_off() {
    for (switch, floor, guess) in [
        (None, 0.3_f32, 0.3_f32),
        (Some(true), 0.3, 0.3),
        (Some(false), 0.7, 0.9),
    ] {
        let (_dir, plain) = state().await;
        let ordinary = model(&plain, |_| {}).await;
        let (_dir, guessing) = state().await;
        let recipe = stored(temperature(0.9), DefaultsOrigin::AutoDetected);
        let guessed = model(&guessing, recipe).await;

        for (state, id, want) in [(&plain, ordinary, floor), (&guessing, guessed, guess)] {
            agentic_sampling(state, switch).await;
            let mut sent_from = Vec::new();
            for (door, chat) in doors(state, id, true).await {
                let body = sent(state, id, chat).await;
                assert!(carries_tools(&body), "{door}");
                sent_from.push((door, body["temperature"].clone()));
            }
            let each = [("the page", json!(want)), ("a paired device", json!(want))];
            assert_eq!(sent_from, each, "stored {switch:?}");
        }
    }
}

/// On a turn with tools a temperature a person chose stands, set on the
/// model or globally; one gglib guessed for an ordinary model is capped like
/// the floor.
#[tokio::test]
async fn a_turn_with_tools_never_lowers_a_temperature_a_person_chose() {
    let (_dir, set) = state().await;
    let on_model = model(&set, stored(temperature(0.9), DefaultsOrigin::User)).await;
    let (_dir, defaulted) = state().await;
    let plain = model(&defaulted, |_| {}).await;
    global(&defaulted, temperature(0.9)).await;
    let (_dir, guess) = state().await;
    let guessed = model(
        &guess,
        stored(temperature(0.9), DefaultsOrigin::AutoDetected),
    )
    .await;

    for (state, id, want) in [
        (&set, on_model, 0.9_f32),
        (&defaulted, plain, 0.9),
        (&guess, guessed, 0.3),
    ] {
        for (door, chat) in doors(state, id, true).await {
            let body = sent(state, id, chat).await;
            assert!(carries_tools(&body), "{door}");
            assert_eq!(body["temperature"], json!(want), "{door}");
        }
    }
}

/// A turn that names nothing and carries no tools sends every sampling
/// parameter `gglib model explain` reports for its model and no other, for
/// an ordinary model and for a reasoning one, each over global defaults.
#[tokio::test]
async fn a_turn_that_names_nothing_sends_what_model_explain_reports() {
    let defaults = InferenceConfig {
        top_k: Some(33),
        presence_penalty: Some(0.4),
        ..temperature(0.42)
    };
    let own = InferenceConfig {
        top_p: Some(0.8),
        ..InferenceConfig::default()
    };
    let (_dir, ordinary) = state().await;
    let plain = model(&ordinary, stored(own, DefaultsOrigin::User)).await;
    global(&ordinary, defaults.clone()).await;
    let (_dir, thinking) = state().await;
    let thinker = model(&thinking, reasoning).await;
    global(&thinking, defaults).await;

    for (state, id) in [(&ordinary, plain), (&thinking, thinker)] {
        let explained = state.models.explain_sampling(id, None).await.unwrap();
        assert_eq!(explained.resolved.temperature, Some(0.42));
        for (door, chat) in doors(state, id, false).await {
            let body = sent(state, id, chat).await;
            assert_eq!(sampling_of(&body), explained.resolved, "{door}");
        }
    }
}
