//! What a chat saved of how it samples reaches the model through this
//! machine's endpoint, the real proxy: its five sampling settings as a
//! client's, which the endpoint honours only while it trusts clients'
//! sampling; its profile on the model's id, which it applies whatever it
//! trusts.

use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::runs::RunStatus;
use gglib_core::domain::{InferenceConfig, InferenceProfile, Machine, ModelRef};
use gglib_core::ports::RunScope;
use serde_json::Value;

use super::hub_turn_endpoint_tests::{frames, llama, status};
use super::hub_turn_tests::turn;
use super::start;
use crate::handlers::agent::endpoint_fixture::daemon;

/// The body the model was sent for one turn on a chat that saved
/// `settings`, with the endpoint trusting clients' sampling as `trusted`
/// says and a profile `warm` configured, which samples at 1.4.
async fn sent(trusted: bool, settings: ConversationSettings) -> Value {
    let (fake, port) = llama().await;
    let d = daemon(port, None).await;
    let warm = InferenceProfile {
        name: "warm".to_owned(),
        description: None,
        config: InferenceConfig {
            temperature: Some(1.4),
            ..InferenceConfig::default()
        },
        list_in_models: false,
    };
    let update = gglib_core::SettingsUpdate {
        trust_client_sampling: Some(Some(trusted)),
        inference_profiles: Some(Some(vec![warm])),
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

/// A temperature the chat saved reaches the model while the endpoint trusts
/// clients' sampling, and not while it does not.
#[tokio::test]
async fn a_chats_saved_sampling_reaches_the_model_only_while_the_endpoint_trusts_clients() {
    let saved = || ConversationSettings {
        temperature: Some(0.3),
        ..ConversationSettings::default()
    };

    let trusted = sent(true, saved()).await;
    let untrusted = sent(false, saved()).await;

    assert!(near(temperature(&trusted), 0.3), "{trusted}");
    assert!(!near(temperature(&untrusted), 0.3), "{untrusted}");
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
