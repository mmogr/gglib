//! What a resumed chat saves, turn after turn, through the loop `gglib chat`
//! composes: the session `prepare` makes of a stored chat, on a `--port`
//! server that answers every turn `A cat.`, each turn run and saved by
//! `run_single_turn` as the REPL runs it.

use gglib_core::domain::agent::MADE_KEYS;
use gglib_core::domain::chat::{
    ConversationSettings, Message, MessageRole, NewConversation, NewMessage,
};

use super::super::resume_settings::tests::chat_args;
use super::super::sight::sight_tests::props_server;
use super::super::{Session, config, prepare};
use super::*;
use crate::bootstrap::{CliContext, test_context};

/// A stored chat on `qwen` with `prompt` as its system prompt, and `rows`.
async fn chat(ctx: &CliContext, prompt: Option<&str>, rows: &[(MessageRole, String)]) -> i64 {
    let history = ctx.app.chat_history();
    let settings = ConversationSettings {
        model_name: Some("qwen".to_owned()),
        ..ConversationSettings::default()
    };
    let conv = NewConversation {
        title: "t".to_owned(),
        system_prompt: prompt.map(str::to_owned),
        settings: Some(settings),
        ..NewConversation::default()
    };
    let id = history.create_conversation(conv).await.expect("saved");
    for (role, content) in rows {
        let row = NewMessage {
            conversation_id: id,
            role: *role,
            content: content.clone(),
            metadata: None,
            images: Vec::new(),
        };
        history.save_message(row).await.expect("saved");
    }
    id
}

/// `gglib chat --continue <id> --port <port>`, then each of `said` typed at
/// its prompt. Answers with the history the session holds at the end.
async fn continued(ctx: &CliContext, id: i64, port: u16, said: &[&str]) -> Vec<AgentMessage> {
    let args = ChatArgs {
        identifier: String::new(),
        continue_id: Some(id),
        port: Some(port),
        no_tools: true,
        ..chat_args()
    };
    let Session {
        params,
        persistence,
        prior_messages: mut messages,
        ..
    } = prepare(ctx, &args).await.expect("resumed");
    let banner = config::BannerInfo {
        quiet: true,
        ..config::BannerInfo::default()
    };
    let agent = config::compose(ctx, &params, None, None, &banner).await;
    let agent = agent.expect("composed");
    for said in said {
        messages.push(AgentMessage::User {
            content: (*said).to_owned(),
            images: Vec::new(),
        });
        let (config, saved_to) = (AgentConfig::default(), persistence.as_ref());
        messages = run_single_turn(&agent, messages, config, false, saved_to).await;
    }
    messages
}

async fn rows(ctx: &CliContext, id: i64) -> Vec<Message> {
    ctx.app.chat_history().get_messages(id).await.expect("read")
}

/// Each row as its role and what it says.
fn said(rows: &[Message]) -> Vec<(MessageRole, String)> {
    rows.iter().map(|r| (r.role, r.content.clone())).collect()
}

/// `turns` turns typed after `stored`: each message, then the server's reply.
fn with_turns(stored: &[Message], turns: &[&str]) -> Vec<(MessageRole, String)> {
    let mut all = said(stored);
    for typed in turns {
        all.push((MessageRole::User, (*typed).to_owned()));
        all.push((MessageRole::Assistant, "A cat.".to_owned()));
    }
    all
}

/// A resumed chat saves what each turn adds, once: the message, then the
/// reply. Nothing it resumed with is saved again, on its first turn or its
/// second, whether or not the chat has a system prompt, which is never a
/// row, and whether or not an older one holds its prompt as a row.
#[tokio::test]
async fn a_resumed_chat_saves_each_turns_rows_once_with_a_system_prompt_or_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let server = props_server("{}");
    let stored = [
        (MessageRole::System, "OLD-PROMPT".to_owned()),
        (MessageRole::User, "first".to_owned()),
        (MessageRole::Assistant, "answer".to_owned()),
    ];

    for (prompt, system_row) in [
        (Some("Be brief."), false),
        (Some("Be brief."), true),
        (None, false),
        (None, true),
    ] {
        let id = chat(&ctx, prompt, &stored[usize::from(!system_row)..]).await;
        let before = rows(&ctx, id).await;

        continued(&ctx, id, server.port, &["second", "third"]).await;

        let after = said(&rows(&ctx, id).await);
        let expected = with_turns(&before, &["second", "third"]);
        assert_eq!(after, expected, "{prompt:?}, {system_row}");
    }
}

/// A chat too long for the loop's context budget, in characters and in
/// messages. The loop prunes what it sends, so the history it hands back is
/// shorter than the chat, and no count of the rows the chat holds. Every
/// turn's message and reply are saved all the same, in order, and the first
/// reply says how much was left out of its request, as the loop reported it.
#[tokio::test]
async fn a_chat_longer_than_the_context_budget_saves_every_turn_in_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let server = props_server("{}");
    let long = |i: usize| {
        let role = [MessageRole::User, MessageRole::Assistant][i % 2];
        (role, format!("{i}: {}", "x".repeat(20_000)))
    };
    let stored: Vec<_> = (0..14).map(long).collect();
    let id = chat(&ctx, Some("Be brief."), &stored).await;
    let before = rows(&ctx, id).await;
    let typed = ["second", "third", "fourth"];

    let held = continued(&ctx, id, server.port, &typed).await;

    let whole = with_turns(&before, &typed);
    assert!(
        held.len() < whole.len(),
        "the loop pruned: the session holds {} messages of the {} said",
        held.len(),
        whole.len()
    );
    let after = rows(&ctx, id).await;
    assert_eq!(said(&after), whole);
    let first_reply = after[before.len() + 1].metadata.as_ref();
    let trimmed = first_reply.and_then(|m| m[MADE_KEYS.trimmed_messages].as_u64());
    assert!(trimmed > Some(0), "{trimmed:?}");
}
