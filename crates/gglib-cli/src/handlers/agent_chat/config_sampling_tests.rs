//! What `gglib chat` and `gglib q` send a model server for sampling: the
//! flags as typed, the stored layers folded beneath them once, where the
//! request is shaped, no flag that fold passes over, and for a server this
//! catalogue does not know the flags alone. Each is read from the request a
//! loopback server received.

use std::path::PathBuf;

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentMessage};
use gglib_core::domain::{DefaultsOrigin, InferenceProfile, ModelCapabilities, NewModel};
use gglib_core::settings::SettingsUpdate;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::tests::chat_args;
use super::*;
use crate::bootstrap::test_context;
use crate::handlers::agent_chat::sight::sight_tests::props_server;
use crate::handlers::agent_chat::{Session, prepare};
use crate::handlers::inference::agent_question::{self, QuestionArgs};
use crate::shared_args::{ContextArgs, SamplingArgs};

/// The catalogue's model, by name.
const MODEL: &str = "served";

fn temperature(value: f32) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(value),
        ..InferenceConfig::default()
    }
}

fn typed(value: f32) -> SamplingArgs {
    SamplingArgs {
        temperature: Some(value),
        ..SamplingArgs::default()
    }
}

/// Add [`MODEL`] to the catalogue: a model that can call tools, tagged
/// `tags`, with `defaults` stored as their origin says they were.
async fn model(
    ctx: &CliContext,
    tags: &[&str],
    defaults: Option<(InferenceConfig, DefaultsOrigin)>,
) {
    let path = PathBuf::from("/models/served.gguf");
    let mut entry = NewModel::new(MODEL.to_owned(), path, 7.0, chrono::Utc::now());
    entry.capabilities =
        ModelCapabilities::SUPPORTS_TOOL_CALLS | ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
    entry.tags = tags.iter().map(|&tag| tag.to_owned()).collect();
    if let Some((config, origin)) = defaults {
        entry.inference_defaults = Some(config);
        entry.defaults_origin = Some(origin);
    }
    ctx.app.models().add(entry).await.expect("added");
}

/// Store `global` as the settings' sampling defaults, and a `coding`
/// profile that sets `profile` when one is given.
async fn settings(ctx: &CliContext, global: Option<InferenceConfig>, profile: Option<f32>) {
    let coding = |value| InferenceProfile {
        name: "coding".to_owned(),
        description: None,
        config: temperature(value),
        list_in_models: false,
    };
    let update = SettingsUpdate {
        inference_defaults: Some(global),
        inference_profiles: Some(Some(profile.map(coding).into_iter().collect())),
        ..SettingsUpdate::default()
    };
    ctx.app.settings().update(update).await.expect("stored");
}

/// Store the agentic sampling switch as `stored`: on, off, or not stored.
async fn agentic_sampling(ctx: &CliContext, stored: Option<bool>) {
    let update = SettingsUpdate {
        agentic_sampling: Some(stored),
        ..SettingsUpdate::default()
    };
    ctx.app.settings().update(update).await.expect("stored");
}

/// The request `gglib chat <args> --port <a recording server>` sends for its
/// first message: the session prepared as `run` prepares it, then composed
/// from its flags, quietly. `run` hands `compose` no flags at all when none
/// was typed, and is not quiet.
async fn chat_sends(ctx: &CliContext, args: ChatArgs) -> Value {
    let server = props_server("{}");
    let args = ChatArgs {
        port: Some(server.port),
        ..args
    };
    let Session { args, params, .. } = prepare(ctx, &args).await.expect("prepared");
    let flags = args.sampling.clone().into_inference_config();
    let banner = BannerInfo {
        quiet: true,
        ..BannerInfo::default()
    };
    let agent = compose(ctx, &params, None, Some(flags), &banner)
        .await
        .expect("composed");
    let (tx, _rx) = mpsc::channel(AGENT_EVENT_CHANNEL_CAPACITY);
    agent
        .run(vec![AgentMessage::user("hi")], AgentConfig::default(), tx)
        .await
        .expect("the turn ends");
    serde_json::from_str(&server.body_of("POST /v1/chat/completions")).expect("a JSON body")
}

/// `gglib chat <identifier>`, with tools unless `no_tools`.
fn chat(identifier: &str, no_tools: bool, sampling: SamplingArgs) -> ChatArgs {
    ChatArgs {
        identifier: identifier.to_owned(),
        no_tools,
        sampling,
        ..chat_args()
    }
}

/// `gglib q -m <identifier> hi`, with every tool or with none.
fn question(dir: &tempfile::TempDir, identifier: &str, tools: bool) -> QuestionArgs {
    // `--file`, so the question does not wait on this process's stdin.
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "{}").expect("written");
    QuestionArgs {
        question: "hi {}".to_owned(),
        model_arg: Some(identifier.to_owned()),
        file: Some(notes.display().to_string()),
        port: None,
        target: Target::Local,
        max_iterations: None,
        tools: if tools {
            Vec::new()
        } else {
            vec!["__none__".to_owned()]
        },
        tool_timeout_ms: None,
        max_parallel: None,
        images: Vec::new(),
        observation_tools: Vec::new(),
        max_observation_steps: None,
        show_prompt: false,
        verbose: false,
        quiet: true,
        sampling: SamplingArgs::default(),
        profile: None,
        context: ContextArgs::default(),
    }
}

/// The request `gglib q <args> --port <a recording server>` sends.
async fn q_sends(ctx: &CliContext, args: QuestionArgs) -> Value {
    let server = props_server("{}");
    let args = QuestionArgs {
        port: Some(server.port),
        ..args
    };
    agent_question::execute(ctx, args).await.expect("answered");
    serde_json::from_str(&server.body_of("POST /v1/chat/completions")).expect("a JSON body")
}

fn carries_tools(body: &Value) -> bool {
    body["tools"]
        .as_array()
        .is_some_and(|tools| !tools.is_empty())
}

/// On a turn with no tools a stored model is sent what its layers resolve
/// to, in the order `gglib model explain` reports: the global value, a value
/// set on the model above it, and a reasoning model's recipe whole.
#[tokio::test]
async fn chat_with_no_tools_sends_what_the_stored_layers_resolve_to() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, &[], None).await;
    settings(&ctx, Some(temperature(0.42)), None).await;
    let body = chat_sends(&ctx, chat(MODEL, true, SamplingArgs::default())).await;
    assert_eq!(body["temperature"], json!(0.42_f32));
    let asked = q_sends(&ctx, question(&dir, MODEL, false)).await;
    assert_eq!(asked["temperature"], json!(0.42_f32), "and `gglib q` alike");

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, &[], Some((temperature(0.55), DefaultsOrigin::User))).await;
    settings(&ctx, Some(temperature(0.42)), None).await;
    let body = chat_sends(&ctx, chat(MODEL, true, SamplingArgs::default())).await;
    assert_eq!(body["temperature"], json!(0.55_f32));

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let recipe = (
        InferenceConfig::reasoning_profile(),
        DefaultsOrigin::AutoDetected,
    );
    model(&ctx, &["reasoning"], Some(recipe)).await;
    let body = chat_sends(&ctx, chat(MODEL, true, SamplingArgs::default())).await;
    let recipe = Value::Object(InferenceConfig::reasoning_profile().to_openai_json_patch());
    for (key, value) in recipe.as_object().expect("an object") {
        assert_eq!(&body[key], value, "{key}");
    }
    assert_eq!(body["max_tokens"], json!(8192));
}

/// `--temperature` is sent as typed, and `--profile` outranks the value set
/// on the model and the global one; a flag typed beside it outranks it.
#[tokio::test]
async fn a_flag_is_sent_as_typed_and_a_profile_outranks_the_model_and_the_global_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, &[], Some((temperature(0.55), DefaultsOrigin::User))).await;
    settings(&ctx, Some(temperature(0.42)), Some(0.15)).await;
    let profiled = |sampling| ChatArgs {
        profile: Some("coding".to_owned()),
        ..chat(MODEL, true, sampling)
    };

    let body = chat_sends(&ctx, chat(MODEL, true, typed(0.9))).await;
    assert_eq!(body["temperature"], json!(0.9_f32));
    let body = chat_sends(&ctx, profiled(SamplingArgs::default())).await;
    assert_eq!(body["temperature"], json!(0.15_f32));
    let body = chat_sends(&ctx, profiled(typed(0.9))).await;
    assert_eq!(body["temperature"], json!(0.9_f32));
}

/// `--presence-penalty` with no `--temperature` is not sent when the profile
/// or the model's own value sets the temperature: the penalty travels with
/// the temperature, the flag is passed over, and the request carries no
/// parameter the stored layers did not resolve. From `gglib chat` and from
/// `gglib q` alike.
#[tokio::test]
async fn a_penalty_typed_without_its_temperature_is_not_sent_beneath_a_layer_that_sets_one() {
    let penalty = || SamplingArgs {
        presence_penalty: Some(1.2),
        ..SamplingArgs::default()
    };
    let sampling = |body: &Value| InferenceConfig::extract_client_sampling(body).0;

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, &[], None).await;
    settings(&ctx, None, Some(0.15)).await;
    let chatted = ChatArgs {
        profile: Some("coding".to_owned()),
        ..chat(MODEL, true, penalty())
    };
    let body = chat_sends(&ctx, chatted).await;
    assert_eq!(sampling(&body), temperature(0.15), "{body}");
    let asked = QuestionArgs {
        sampling: penalty(),
        profile: Some("coding".to_owned()),
        ..question(&dir, MODEL, false)
    };
    let body = q_sends(&ctx, asked).await;
    assert_eq!(sampling(&body), temperature(0.15), "{body}");

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, &[], Some((temperature(0.55), DefaultsOrigin::User))).await;
    let body = chat_sends(&ctx, chat(MODEL, true, penalty())).await;
    assert_eq!(sampling(&body), temperature(0.55), "{body}");
    let asked = QuestionArgs {
        sampling: penalty(),
        ..question(&dir, MODEL, false)
    };
    let body = q_sends(&ctx, asked).await;
    assert_eq!(sampling(&body), temperature(0.55), "{body}");
}

/// An ordinary model with nothing chosen is sent the floor's 0.7, and 0.3 on
/// a turn with tools, from `gglib chat` and from `gglib q` alike: the
/// floor's value reaches the fold as the floor's, not as one a person typed.
#[tokio::test]
async fn an_ordinary_model_with_nothing_chosen_is_capped_on_a_turn_with_tools() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    model(&ctx, &[], None).await;

    let plain = chat_sends(&ctx, chat(MODEL, true, SamplingArgs::default())).await;
    assert!(!carries_tools(&plain));
    assert_eq!(plain["temperature"], json!(0.7_f32));
    let tooled = chat_sends(&ctx, chat(MODEL, false, SamplingArgs::default())).await;
    assert!(carries_tools(&tooled));
    assert_eq!(tooled["temperature"], json!(0.3_f32));

    let plain = q_sends(&ctx, question(&dir, MODEL, false)).await;
    assert!(!carries_tools(&plain));
    assert_eq!(plain["temperature"], json!(0.7_f32));
    let tooled = q_sends(&ctx, question(&dir, MODEL, true)).await;
    assert!(carries_tools(&tooled));
    assert_eq!(tooled["temperature"], json!(0.3_f32));
}

/// `gglib chat` and `gglib q` on a model in the catalogue follow this
/// machine's agentic sampling switch. Stored off, a turn with tools is sent
/// the floor's 0.7, the temperature it resolves to; stored on, or not stored,
/// 0.3.
#[tokio::test]
async fn a_catalogued_models_turn_with_tools_is_capped_unless_agentic_sampling_is_stored_off() {
    for (stored, want) in [(None, 0.3_f32), (Some(true), 0.3), (Some(false), 0.7)] {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_context(dir.path()).await;
        model(&ctx, &[], None).await;
        agentic_sampling(&ctx, stored).await;

        let chatted = chat_sends(&ctx, chat(MODEL, false, SamplingArgs::default())).await;
        let asked = q_sends(&ctx, question(&dir, MODEL, true)).await;
        assert!(carries_tools(&chatted) && carries_tools(&asked));
        let sent = [&chatted["temperature"], &asked["temperature"]];
        assert_eq!(sent, [&json!(want); 2], "chat, then q; stored {stored:?}");
    }
}

/// On a turn with tools a temperature a person chose stands, whichever layer
/// they chose it in, and so does a reasoning model's recipe; a recipe gglib
/// guessed for an ordinary model is capped like the floor.
#[tokio::test]
async fn a_turn_with_tools_never_lowers_a_temperature_a_person_chose() {
    let sent = async |defaults, tags: &[&str], global, profile: Option<f32>, flag: Option<f32>| {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_context(dir.path()).await;
        model(&ctx, tags, defaults).await;
        settings(&ctx, global, profile).await;
        let args = ChatArgs {
            profile: profile.map(|_| "coding".to_owned()),
            ..chat(MODEL, false, flag.map(typed).unwrap_or_default())
        };
        let body = chat_sends(&ctx, args).await;
        assert!(carries_tools(&body));
        body["temperature"].clone()
    };
    let hot = json!(0.9_f32);
    let stored = |origin| Some((temperature(0.9), origin));

    assert_eq!(sent(None, &[], None, None, Some(0.9)).await, hot);
    assert_eq!(sent(None, &[], None, Some(0.9), None).await, hot);
    assert_eq!(
        sent(stored(DefaultsOrigin::User), &[], None, None, None).await,
        hot
    );
    assert_eq!(
        sent(None, &[], Some(temperature(0.9)), None, None).await,
        hot
    );
    let guessed = stored(DefaultsOrigin::AutoDetected);
    assert_eq!(sent(guessed, &["reasoning"], None, None, None).await, hot);
    let guessed = stored(DefaultsOrigin::AutoDetected);
    assert_eq!(sent(guessed, &[], None, None, None).await, json!(0.3_f32));
}

/// A server on `--port` whose model this catalogue does not hold is sent the
/// flags and nothing stored: no global value beneath them, the floor's 0.7,
/// and 0.3 on a turn with tools, with the agentic sampling switch stored
/// off. The switch is for this catalogue's models.
#[tokio::test]
async fn a_server_this_catalogue_does_not_know_is_sent_the_flags_and_nothing_stored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    settings(&ctx, Some(temperature(0.42)), None).await;
    agentic_sampling(&ctx, Some(false)).await;

    let plain = chat_sends(&ctx, chat("stranger", true, SamplingArgs::default())).await;
    assert_eq!(plain["temperature"], json!(0.7_f32));
    let tooled = chat_sends(&ctx, chat("stranger", false, SamplingArgs::default())).await;
    assert_eq!(tooled["temperature"], json!(0.3_f32));
    let flagged = chat_sends(&ctx, chat("stranger", false, typed(0.9))).await;
    assert_eq!(flagged["temperature"], json!(0.9_f32));
    let asked = q_sends(&ctx, question(&dir, "stranger", false)).await;
    assert_eq!(asked["temperature"], json!(0.7_f32));
    let asked = q_sends(&ctx, question(&dir, "stranger", true)).await;
    assert_eq!(asked["temperature"], json!(0.3_f32), "and `gglib q` alike");
}
