//! What a chat saved of how it samples reaches the model through this
//! machine's endpoint, the real proxy: its token limit as a client's
//! budget, and its profile on the model's id, both of which the endpoint
//! applies whatever it trusts.

use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::runs::RunStatus;
use gglib_core::domain::{InferenceConfig, InferenceProfile, Machine, ModelRef};
use gglib_core::ports::RunScope;
use serde_json::Value;

use super::hub_turn_endpoint_tests::{frames, llama, status};
use super::hub_turn_tests::turn;
use super::start;
use crate::handlers::agent::endpoint_fixture::daemon_started;
use gglib_runtime::proxy::ProxyConfig;

/// A profile `name` configured with `config`.
fn profile(name: &str, config: InferenceConfig) -> InferenceProfile {
    InferenceProfile {
        name: name.to_owned(),
        description: None,
        config,
        list_in_models: false,
    }
}

/// The body the model was sent for one turn on a chat that saved
/// `settings`, with the endpoint trusting clients' sampling as `trusted`
/// says and a profile `warm` configured, which samples at 1.4.
async fn sent(trusted: bool, settings: ConversationSettings) -> Value {
    let warm = profile(
        "warm",
        InferenceConfig {
            temperature: Some(1.4),
            ..InferenceConfig::default()
        },
    );
    sent_with(trusted, settings, vec![warm], |_| {}).await
}

/// [`sent`], with `profiles` configured and the endpoint started as `started`
/// leaves it (`gglib serve --profile`).
async fn sent_with(
    trusted: bool,
    settings: ConversationSettings,
    profiles: Vec<InferenceProfile>,
    started: impl FnOnce(&mut ProxyConfig),
) -> Value {
    let (fake, port) = llama().await;
    let d = daemon_started(port, None, started).await;
    let update = gglib_core::SettingsUpdate {
        trust_client_sampling: Some(Some(trusted)),
        inference_profiles: Some(Some(profiles)),
        ..gglib_core::SettingsUpdate::default()
    };
    d.state.core.settings().update(update).await.unwrap();
    let settings = ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Local,
            id: d.model,
        }),
        ..settings
    };
    let chat = d
        .state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(settings),
        })
        .await
        .unwrap();
    let local = RunScope::Local;
    start(&d.state, local.clone(), "s1", turn(chat, "hi"), None)
        .await
        .unwrap();
    let said = frames(&d.state, &local, "s1").await;
    assert_eq!(
        status(&d.state, &local, "s1"),
        RunStatus::Completed,
        "{said:?}"
    );
    d.stop().await;
    let body = fake
        .seen
        .lock()
        .unwrap()
        .iter()
        .find(|request| request.uri == "/v1/chat/completions")
        .map(|request| request.body.clone())
        .expect("the turn reached the model");
    serde_json::from_str(&body).unwrap()
}

/// The temperature `body` reached the model with, if any.
fn temperature(body: &Value) -> Option<f64> {
    body["temperature"].as_f64()
}

fn near(got: Option<f64>, want: f64) -> bool {
    got.is_some_and(|got| (got - want).abs() < 1e-4)
}

/// The token limit a chat saved reaches the model whether or not the
/// endpoint trusts clients' sampling: it is a budget.
#[tokio::test]
async fn a_chats_saved_token_limit_reaches_the_model_whatever_the_endpoint_trusts() {
    let saved = || ConversationSettings {
        max_tokens: Some(321),
        ..ConversationSettings::default()
    };

    let trusted = sent(true, saved()).await;
    let untrusted = sent(false, saved()).await;

    assert_eq!(trusted["max_tokens"], Value::from(321), "{trusted}");
    assert_eq!(untrusted["max_tokens"], Value::from(321), "{untrusted}");
}

/// The turn's limit is folded with the profile the endpoint applies: the
/// chat's, not the endpoint's default. A chat profile that names a
/// `max_tokens` reaches the model with it, where the default names none;
/// and where only the default names one, the chat's profile, naming none,
/// leaves the turn's own limit to be written.
#[tokio::test]
async fn the_turns_limit_is_folded_with_the_chats_profile_not_the_default() {
    let limited = |name: &str, tokens| {
        profile(
            name,
            InferenceConfig {
                max_tokens: Some(tokens),
                ..InferenceConfig::default()
            },
        )
    };
    let plain = |name: &str| profile(name, InferenceConfig::default());
    let on = |name: &str| {
        let name = name.to_owned();
        move |config: &mut ProxyConfig| config.default_profile = Some(name)
    };
    let chat = |name: &str| ConversationSettings {
        profile: Some(name.to_owned()),
        ..ConversationSettings::default()
    };

    let chats_own = sent_with(
        false,
        chat("long"),
        vec![limited("long", 700), plain("free")],
        on("free"),
    )
    .await;
    let defaults_only = sent_with(
        false,
        chat("free"),
        vec![limited("short", 512), plain("free")],
        on("short"),
    )
    .await;

    assert_eq!(chats_own["max_tokens"], Value::from(700), "{chats_own}");
    assert_eq!(
        defaults_only["max_tokens"],
        Value::from(16_384),
        "{defaults_only}"
    );
}

/// A profile the chat saved that is no longer configured is left off: the
/// turn runs without it, rather than being refused for it.
#[tokio::test]
async fn a_saved_profile_no_longer_configured_is_left_off_and_the_turn_runs() {
    let gone = ConversationSettings {
        profile: Some("deleted".to_owned()),
        ..ConversationSettings::default()
    };

    let body = sent(false, gone).await;

    assert!(!near(temperature(&body), 1.4), "{body}");
}

/// The profile a chat saved reaches the model as that profile's sampling,
/// though the endpoint trusts no client's.
#[tokio::test]
async fn a_chats_saved_profile_reaches_the_model_whatever_the_endpoint_trusts() {
    let profiled = ConversationSettings {
        profile: Some("warm".to_owned()),
        ..ConversationSettings::default()
    };

    let body = sent(false, profiled).await;
    let plain = sent(false, ConversationSettings::default()).await;

    assert!(near(temperature(&body), 1.4), "{body}");
    assert!(!near(temperature(&plain), 1.4), "{plain}");
}
