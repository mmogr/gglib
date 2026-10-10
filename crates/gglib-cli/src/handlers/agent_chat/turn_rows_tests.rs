//! The rows a chat turn saves, through `run_single_turn` over a database of
//! its own, with a scripted loop as the daemon's run tests have.
//!
//! `transcript_turn.json`, beside the writer in `gglib-app-services`, holds
//! one turn as the events its loop sends and the rows it is saved as. The
//! daemon's agent run is held to the same file by its own test
//! (`run_rows_tests`), so the two surfaces save a turn alike.

use async_trait::async_trait;
use gglib_core::domain::agent::{
    AssistantContent, ContextReading, ToolCall, ToolResult, TurnUsage,
};
use gglib_core::domain::chat::{Message, MessageRole};
use gglib_core::ports::{AgentError, AgentRunOutput};
use serde_json::{Value, json};

use super::*;
use crate::bootstrap::{CliContext, test_context};
use crate::target::TurnModel;

const TURN: &str = include_str!("../../../../gglib-app-services/src/transcript_turn.json");

/// A loop that sends its events, then ends: with the history `hands_back`
/// makes of the messages it was given, or failed when there is none.
struct Scripted {
    events: Vec<AgentEvent>,
    hands_back: Option<fn(Vec<AgentMessage>) -> Vec<AgentMessage>>,
}

#[async_trait]
impl AgentLoopPort for Scripted {
    async fn run(
        &self,
        messages: Vec<AgentMessage>,
        _config: AgentConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<AgentRunOutput, AgentError> {
        for event in &self.events {
            let _ = tx.send(event.clone()).await;
        }
        let Some(hands_back) = self.hands_back else {
            return Err(AgentError::MaxIterationsReached(1));
        };
        Ok(AgentRunOutput {
            answer: String::new(),
            history: hands_back(messages),
            total_iterations: 1,
            total_completion_tokens: None,
        })
    }
}

fn user(content: &str) -> AgentMessage {
    AgentMessage::User {
        content: content.to_owned(),
        images: Vec::new(),
    }
}

fn text(content: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        content: content.to_owned(),
    }
}

fn call() -> ToolCall {
    ToolCall {
        id: "c1".to_owned(),
        name: "read_file".to_owned(),
        arguments: json!({ "path": "src/main.rs" }),
    }
}

fn called() -> [AgentEvent; 3] {
    [
        AgentEvent::ToolCallStart {
            tool_call: call(),
            display_name: "Read File".to_owned(),
            args_summary: Some("src/main.rs".to_owned()),
        },
        AgentEvent::ToolCallComplete {
            tool_name: "read_file".to_owned(),
            result: ToolResult::text("c1".to_owned(), "fn main() {}".to_owned(), true),
            wait_ms: 0,
            execute_duration_ms: 1,
            display_name: "Read File".to_owned(),
            duration_display: "1ms".to_owned(),
        },
        AgentEvent::IterationComplete {
            iteration: 1,
            tool_calls: 1,
        },
    ]
}

fn usage(prompt: u32, written: u32, stopped: &str, trimmed: usize) -> AgentEvent {
    AgentEvent::TurnUsage(TurnUsage {
        prompt_tokens: Some(prompt),
        cached_tokens: Some(10),
        completion_tokens: Some(written),
        duration_ms: 700,
        writing_ms: Some(500),
        finish_reason: Some(stopped.to_owned()),
        reading: ContextReading::new(None, trimmed),
        ..TurnUsage::default()
    })
}

/// The file's turn: it reasons, calls a tool and answers.
fn turn() -> Vec<AgentEvent> {
    let reasoning = AgentEvent::ReasoningDelta {
        content: "The file will say.".to_owned(),
    };
    let asked = [
        reasoning,
        text("Reading "),
        text("the file."),
        usage(40, 12, "tool_calls", 2),
    ];
    let answered = [
        text("It is an empty main."),
        usage(64, 7, "stop", 0),
        AgentEvent::FinalAnswer {
            content: "It is an empty `main`.".to_owned(),
        },
    ];
    [asked.to_vec(), called().to_vec(), answered.to_vec()].concat()
}

/// A saved row as the file spells one: all of it but its ids and its time.
fn spelled(row: &Message) -> Value {
    json!({
        "role": row.role,
        "content": row.content,
        "metadata": row.metadata,
        "images": row.images.len(),
    })
}

/// A new conversation of a session on `qwen`, as `gglib chat qwen` makes.
async fn session(ctx: &CliContext) -> Conversation<'_> {
    let made_by = TurnModel::here("qwen".to_owned(), None).made_by();
    let made = Conversation::create(ctx.app.chat_history(), None, None, made_by).await;
    made.expect("a conversation")
}

/// One turn of the REPL's on `said`: the message joins the history, and the
/// turn runs and is saved to `conversation`.
async fn say(
    agent: Scripted,
    messages: &mut Vec<AgentMessage>,
    conversation: &Conversation<'_>,
    said: &str,
) {
    let agent: Arc<dyn AgentLoopPort> = Arc::new(agent);
    messages.push(user(said));
    let turn = std::mem::take(messages);
    let saved_to = Some(conversation);
    *messages = run_single_turn(&agent, turn, AgentConfig::default(), false, saved_to, true).await;
}

async fn rows(ctx: &CliContext, id: i64) -> Vec<Message> {
    ctx.app.chat_history().get_messages(id).await.expect("read")
}

/// The file's turn, finished and stopped before its answer, is saved by a
/// chat as the rows the file holds: the rows the daemon saves for it.
#[tokio::test]
async fn a_turn_is_saved_as_the_rows_the_file_holds_finished_or_not() {
    let pinned: Value = serde_json::from_str(TURN).expect("the file parses");
    let events: Value = turn()
        .iter()
        .map(|event| serde_json::to_value(event).expect("an event"))
        .collect();
    assert_eq!(events, pinned["events"], "the file's own turn");
    let mut stopped = turn();
    stopped.pop();
    let finished = Scripted {
        events: turn(),
        hands_back: Some(|given| given),
    };
    let failed = Scripted {
        events: stopped,
        hands_back: None,
    };

    for (agent, held) in [(finished, "rows"), (failed, "rows_when_stopped")] {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = test_context(dir.path()).await;
        let conversation = session(&ctx).await;

        say(agent, &mut Vec::new(), &conversation, "PROMPT-SECRET").await;

        let saved: Value = rows(&ctx, conversation.id)
            .await
            .iter()
            .map(spelled)
            .collect();
        assert_eq!(saved, pinned[held], "{held}: {saved:#}");
    }
}

/// What the loop hands back once it has pruned: of what it was given, only
/// the user's message, then what its turn added.
fn pruned(given: Vec<AgentMessage>) -> Vec<AgentMessage> {
    let assistant = |text: &str, tool_calls| AgentMessage::Assistant {
        content: AssistantContent {
            text: Some(text.to_owned()),
            tool_calls,
        },
    };
    let result = AgentMessage::Tool {
        tool_call_id: "c1".to_owned(),
        content: "fn main() {}".to_owned(),
    };
    let asked = given.into_iter().next_back();
    let added = [
        assistant("Looking.", vec![call()]),
        result,
        assistant("Empty.", Vec::new()),
    ];
    asked.into_iter().chain(added).collect()
}

/// A loop that prunes hands back a history no longer than the one before
/// it, turn after turn. Every turn's rows are saved all the same, in the
/// order they were said: the message, the call, its result, the answer.
#[tokio::test]
async fn every_turn_is_saved_in_order_when_the_loop_hands_back_a_pruned_history() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let conversation = session(&ctx).await;
    let mut messages = Vec::new();
    let answer = AgentEvent::FinalAnswer {
        content: "Empty.".to_owned(),
    };
    let events = [vec![text("Looking.")], called().to_vec(), vec![answer]].concat();

    for said in ["one", "two", "three"] {
        let agent = Scripted {
            events: events.clone(),
            hands_back: Some(pruned),
        };
        say(agent, &mut messages, &conversation, said).await;
        assert_eq!(messages.len(), 4, "the loop keeps one turn");
    }

    let saved = rows(&ctx, conversation.id).await;
    let said: Vec<(MessageRole, &str)> = saved.iter().map(|r| (r.role, &*r.content)).collect();
    let turn = |asked| {
        [
            (MessageRole::User, asked),
            (MessageRole::Assistant, "Looking."),
            (MessageRole::Tool, "fn main() {}"),
            (MessageRole::Assistant, "Empty."),
        ]
    };
    assert_eq!(said, [turn("one"), turn("two"), turn("three")].concat());
    let calls = |row: &Message| {
        row.metadata
            .as_ref()
            .map(|m| m["tool_calls"][0]["id"].clone())
    };
    assert_eq!(
        calls(&saved[5]),
        Some(json!("c1")),
        "the second turn's call"
    );
    assert_eq!(saved[6].metadata, Some(json!({ "tool_call_id": "c1" })));
}

/// A turn with no conversation to save to runs, and writes nothing.
#[tokio::test]
async fn a_turn_with_no_conversation_saves_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let canary = session(&ctx).await;
    let agent: Arc<dyn AgentLoopPort> = Arc::new(Scripted {
        events: turn(),
        hands_back: Some(|given| given),
    });

    let after = run_single_turn(
        &agent,
        vec![user("hi")],
        AgentConfig::default(),
        false,
        None,
        true,
    );

    assert_eq!(after.await.len(), 1);
    assert!(rows(&ctx, canary.id).await.is_empty());
    let chats = ctx.app.chat_history().list_conversations().await;
    assert_eq!(chats.expect("read").len(), 1);
}

/// A turn that answers a question already saved, as `/retry` and `/edit`
/// start one, saves its reply after the question and no question of its own.
#[tokio::test]
async fn a_turn_that_answers_a_saved_question_saves_only_its_reply() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let conversation = session(&ctx).await;
    let mut asked = Vec::new();
    say(
        Scripted {
            events: Vec::new(),
            hands_back: None,
        },
        &mut asked,
        &conversation,
        "hi",
    )
    .await;
    let agent: Arc<dyn AgentLoopPort> = Arc::new(Scripted {
        events: vec![AgentEvent::FinalAnswer {
            content: "Hello.".to_owned(),
        }],
        hands_back: Some(|given| given),
    });

    let saved_to = Some(&conversation);
    run_single_turn(
        &agent,
        vec![user("hi")],
        AgentConfig::default(),
        false,
        saved_to,
        false,
    )
    .await;

    let saved = rows(&ctx, conversation.id).await;
    let said: Vec<(MessageRole, &str)> = saved.iter().map(|r| (r.role, &*r.content)).collect();
    assert_eq!(said[..1], [(MessageRole::User, "hi")]);
    assert_eq!(
        said.iter()
            .filter(|(role, _)| *role == MessageRole::User)
            .count(),
        1
    );
    assert_eq!(said.last(), Some(&(MessageRole::Assistant, "Hello.")));
}

/// A send switches Draw off again: the message after the one that drew is
/// offered no image tool.
#[tokio::test]
async fn a_send_switches_draw_off_again() {
    use gglib_core::ports::{ImageBatch, ImageError, ImageProgress, ImageRequest};

    #[derive(Debug)]
    struct CanDraw;

    #[async_trait::async_trait]
    impl gglib_core::ports::ImageGenerationPort for CanDraw {
        async fn generate(
            &self,
            _request: ImageRequest,
            _progress: tokio::sync::mpsc::Sender<ImageProgress>,
        ) -> Result<ImageBatch, ImageError> {
            unreachable!("the scripted loop draws nothing")
        }

        async fn drawing_model(&self) -> Result<String, ImageError> {
            Ok("sdxl".to_owned())
        }
    }

    let agent: Arc<dyn AgentLoopPort> = Arc::new(Scripted {
        events: turn(),
        hands_back: Some(|given| given),
    });
    let draw = super::DrawSwitch::through(Arc::new(CanDraw));
    draw.arm().await;
    assert!(draw.is_armed());

    let config = AgentConfig::default();
    super::send(
        &agent,
        vec![user("draw a fox")],
        config,
        false,
        None,
        true,
        &draw,
    )
    .await;

    assert!(!draw.is_armed());
}
