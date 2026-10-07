//! A chat the CLI resumes runs as it remembers its Thinking choice, by the
//! rule the daemon reads a turn by: `gglib chat --continue` on a `--port`
//! server, and the budget its turn sends that server.

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

/// What conversation `id` remembers of thinking.
async fn remembered(ctx: &CliContext, id: i64) -> Option<Thinking> {
    let saved = ctx.app.chat_history().get_conversation(id).await;
    saved.unwrap().unwrap().settings.and_then(|s| s.thinking)
}

/// `gglib chat --continue <id> --port … [--reasoning-budget-tokens <typed>]`
/// and one message: the thinking budget the turn's request carries.
async fn budget_sent(ctx: &CliContext, id: i64, typed: Option<i32>) -> Value {
    let server = props_server("{}");
    let args = ChatArgs {
        identifier: String::new(),
        continue_id: Some(id),
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
        prior_messages: mut messages,
        ..
    } = prepare(ctx, &args).await.expect("resumed");
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
    sent["reasoning_budget_tokens"].clone()
}

/// The budget that stops thinking, whatever `--reasoning-budget-tokens`
/// says, and the chat goes on remembering `off`.
#[tokio::test]
async fn a_resumed_chat_switched_off_runs_with_thinking_off_whatever_budget_is_typed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    for typed in [None, Some(4096), Some(-1)] {
        let id = chat(&ctx, Some(Thinking::Off)).await;

        assert_eq!(
            budget_sent(&ctx, id, typed).await,
            json!(0),
            "typed {typed:?}"
        );
        assert_eq!(remembered(&ctx, id).await, Some(Thinking::Off));
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

        assert_eq!(budget_sent(&ctx, id, typed).await, sent, "typed {typed:?}");
        assert_eq!(remembered(&ctx, id).await, None);
    }
}
