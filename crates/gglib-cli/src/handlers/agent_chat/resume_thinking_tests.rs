//! A CLI chat's Thinking choice, by the rule the daemon reads a turn by: a
//! resumed chat runs as it remembers, and `--thinking` names the choice for
//! the session and has the chat remember it. `gglib chat` on a `--port`
//! server, the budget its turn sends that server, and what the chat stores.

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::Thinking;
use gglib_core::domain::agent::{AgentConfig, AgentMessage};
use gglib_core::domain::chat::NewConversation;
use serde_json::{Value, json};

use super::super::sight::sight_tests::props_server;
use super::super::{Session, config, prepare};
use super::tests::chat_args;
use super::*;
use crate::bootstrap::{CliContext, test_context};
use crate::shared_args::SamplingArgs;

const OFF: Option<Thinking> = Some(Thinking::Off);
/// What `--thinking on` says: the turn's `default`.
const ON: Option<Thinking> = Some(Thinking::Default);

/// A stored chat on `qwen` that remembers `thinking`.
async fn chat(ctx: &CliContext, thinking: Option<Thinking>) -> i64 {
    let settings = ConversationSettings {
        model_name: Some("qwen".to_owned()),
        thinking,
        ..ConversationSettings::default()
    };
    ctx.app
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            settings: Some(settings),
            ..NewConversation::default()
        })
        .await
        .expect("saved")
}

/// The settings conversation `id` stores, as every surface reads them.
async fn stored(ctx: &CliContext, id: i64) -> Value {
    let saved = ctx.app.chat_history().get_conversation(id).await;
    serde_json::to_value(saved.unwrap().unwrap().settings).expect("settings")
}

/// What conversation `id` remembers of thinking.
async fn remembered(ctx: &CliContext, id: i64) -> Option<Thinking> {
    let saved = ctx.app.chat_history().get_conversation(id).await;
    saved.unwrap().unwrap().settings.and_then(|s| s.thinking)
}

/// `gglib chat [qwen | --continue <id>] --port … [--thinking <said>]
/// [--reasoning-budget-tokens <typed>]` and one message: the thinking budget
/// the turn's request carries, and the id of the chat the session saves to.
async fn turn(
    ctx: &CliContext,
    id: Option<i64>,
    said: Option<Thinking>,
    typed: Option<i32>,
) -> (Value, i64) {
    let server = props_server("{}");
    let args = ChatArgs {
        identifier: id.map_or_else(|| "qwen".to_owned(), |_| String::new()),
        continue_id: id,
        thinking: said,
        port: Some(server.port),
        no_tools: true,
        sampling: SamplingArgs {
            reasoning_budget_tokens: typed,
            ..SamplingArgs::default()
        },
        ..chat_args()
    };
    let Session {
        args,
        params,
        persistence,
        prior_messages: mut messages,
        ..
    } = prepare(ctx, &args).await.expect("prepared");
    let banner = config::BannerInfo {
        quiet: true,
        ..config::BannerInfo::default()
    };
    // The session's sampling, as `run` hands it to `compose`.
    let sampling = Some(args.sampling.into_inference_config());
    let agent = config::compose(ctx, &params, None, sampling, &banner).await;
    let agent = agent.expect("composed");
    messages.push(AgentMessage::User {
        content: "hello".to_owned(),
        images: Vec::new(),
    });
    let (tx, _events) = tokio::sync::mpsc::channel(AGENT_EVENT_CHANNEL_CAPACITY);
    let ran = agent.run(messages, AgentConfig::default(), tx).await;
    ran.expect("answered");
    let sent: Value =
        serde_json::from_str(&server.body_of("POST /v1/chat/completions")).expect("a JSON body");
    let budget = sent["reasoning_budget_tokens"].clone();
    (budget, persistence.expect("saved").id)
}

/// The budget a resume of chat `id` sends.
async fn budget_sent(
    ctx: &CliContext,
    id: i64,
    said: Option<Thinking>,
    typed: Option<i32>,
) -> Value {
    turn(ctx, Some(id), said, typed).await.0
}

/// The budget that stops thinking, whatever `--reasoning-budget-tokens`
/// says, and the chat goes on remembering `off`.
#[tokio::test]
async fn a_resumed_chat_switched_off_runs_with_thinking_off_whatever_budget_is_typed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    for typed in [None, Some(4096), Some(-1)] {
        let id = chat(&ctx, OFF).await;

        assert_eq!(
            budget_sent(&ctx, id, None, typed).await,
            json!(0),
            "typed {typed:?}"
        );
        assert_eq!(remembered(&ctx, id).await, OFF);
    }
}

/// With nothing remembered the budget is the command line's own, and none
/// when it names none.
#[tokio::test]
async fn a_resumed_chat_that_remembers_nothing_runs_with_the_budget_typed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    for (typed, sent) in [
        (None, Value::Null),
        (Some(4096), json!(4096)),
        (Some(0), json!(0)),
    ] {
        let id = chat(&ctx, None).await;

        let budget = budget_sent(&ctx, id, None, typed).await;

        assert_eq!(budget, sent, "typed {typed:?}");
        assert_eq!(remembered(&ctx, id).await, None);
    }
}

/// `--thinking on` on a chat switched off: the budget is the command line's
/// own again, and the chat no longer remembers `off`.
#[tokio::test]
async fn thinking_on_runs_a_chat_switched_off_with_the_budget_typed_and_the_chat_forgets_off() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    for (typed, sent) in [
        (None, Value::Null),
        (Some(4096), json!(4096)),
        (Some(0), json!(0)),
    ] {
        let id = chat(&ctx, OFF).await;

        let budget = budget_sent(&ctx, id, ON, typed).await;

        assert_eq!(budget, sent, "typed {typed:?}");
        assert_eq!(remembered(&ctx, id).await, None, "typed {typed:?}");
    }
}

/// `--thinking off` on a chat that thinks: the budget that stops thinking,
/// whatever is typed, and the chat remembers `off`.
#[tokio::test]
async fn thinking_off_runs_a_chat_that_thinks_with_thinking_off_and_the_chat_remembers_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    for typed in [None, Some(4096), Some(-1)] {
        let id = chat(&ctx, None).await;

        let budget = budget_sent(&ctx, id, OFF, typed).await;

        assert_eq!(budget, json!(0), "typed {typed:?}");
        assert_eq!(remembered(&ctx, id).await, OFF, "typed {typed:?}");
    }
}

/// A new chat named `off` stores the choice as the page's switch and a
/// paired device's turn store it and read it back, `thinking: "off"`, beside
/// what the session saved. Named `on`, or not named, it stores no choice.
#[tokio::test]
async fn a_new_chat_named_off_stores_the_choice_and_one_named_on_or_not_named_stores_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    for (said, sent, kept) in [
        (OFF, json!(0), Some(json!("off"))),
        (ON, json!(4096), None),
        (None, json!(4096), None),
    ] {
        let (budget, id) = turn(&ctx, None, said, Some(4096)).await;

        assert_eq!(budget, sent, "said {said:?}");
        let settings = stored(&ctx, id).await;
        assert_eq!(settings.get("thinking"), kept.as_ref(), "said {said:?}");
        assert_eq!(settings["model_name"], json!("qwen"), "said {said:?}");
    }
}

/// A resume that moves the chat to another model replaces its settings
/// whole, with what the chat remembered before the session. The choice named
/// is written after, so it is the one stored, beside the model.
#[tokio::test]
async fn a_choice_named_on_a_resume_that_moves_to_another_model_is_the_one_stored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let llama = gglib_core::domain::NewModel::new(
        "llama".to_owned(),
        dir.path().join("llama.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    ctx.app.models().add(llama).await.expect("registered");
    let id = chat(&ctx, OFF).await;
    let args = ChatArgs {
        identifier: "llama".to_owned(),
        continue_id: Some(id),
        thinking: ON,
        ..chat_args()
    };

    prepare(&ctx, &args).await.expect("continued on llama");

    let settings = stored(&ctx, id).await;
    assert_eq!(settings["model_name"], json!("llama"));
    assert_eq!(settings.get("thinking"), None);
}
