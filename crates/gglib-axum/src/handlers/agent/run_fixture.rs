//! A scripted agent loop for the agent run's tests, and a daemon context of
//! its own for any handler's.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage, ToolCall, ToolResult};
use gglib_core::domain::chat::{Message, NewConversation};
use gglib_core::domain::runs::RunInfo;
use gglib_core::ports::{AgentError, AgentLoopPort, AgentRunOutput, RunScope, RunsPort as _};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::compose::Prepared;
use super::launch::launch;
use crate::state::AppState;

pub(super) const LOCAL: RunScope = RunScope::Local;

pub(super) enum End {
    Finish,
    Fail,
    /// Fails as the loop does when llama-server goes silent mid-reply.
    Stalled,
    Hang,
    Panic,
}

/// A loop that sends its events, then ends as told. Counts its own drops.
pub(super) struct Scripted {
    events: Vec<AgentEvent>,
    end: End,
    dropped: Arc<AtomicUsize>,
    /// How long it waits after each event.
    pace: Duration,
}

pub(super) struct Dropped(Arc<AtomicUsize>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl AgentLoopPort for Scripted {
    async fn run(
        &self,
        _messages: Vec<AgentMessage>,
        _config: AgentConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<AgentRunOutput, AgentError> {
        let _guard = Dropped(Arc::clone(&self.dropped));
        for event in &self.events {
            let _ = tx.send(event.clone()).await;
            tokio::time::sleep(self.pace).await;
        }
        match self.end {
            End::Finish => Ok(AgentRunOutput {
                answer: String::new(),
                history: Vec::new(),
                total_iterations: 1,
                total_completion_tokens: None,
            }),
            End::Fail => Err(AgentError::LoopDetected {
                signature: "SIGNATURE-SECRET".to_owned(),
            }),
            End::Stalled => Err(AgentError::Internal(
                "stream collection error: llama-server sent nothing for 300s".to_owned(),
            )),
            End::Hang => std::future::pending().await,
            End::Panic => panic!("the scripted loop panicked"),
        }
    }
}

pub(in crate::handlers) async fn state() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::bootstrap::bootstrap(crate::ServerConfig {
        base_port: Some(19_200),
        llama_server_path: "/nonexistent/llama-server".into(),
        sd_server_path: "/nonexistent/sd-server".into(),
        max_concurrent_agent_loops: 1,
        db_path: Some(dir.path().join("gglib.db")),
        device_keys_path: Some(dir.path().join("remote_devices")),
    })
    .await
    .expect("bootstrap an isolated context");
    (dir, Arc::new(state))
}

pub(super) fn user() -> AgentMessage {
    AgentMessage::User {
        content: "PROMPT-SECRET".to_owned(),
        images: Vec::new(),
    }
}

pub(super) fn prepared(events: Vec<AgentEvent>, end: End) -> (Prepared, Arc<AtomicUsize>) {
    paced(events, end, Duration::ZERO)
}

/// As [`prepared`], waiting `pace` after each event.
pub(super) fn paced(
    events: Vec<AgentEvent>,
    end: End,
    pace: Duration,
) -> (Prepared, Arc<AtomicUsize>) {
    let dropped = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel(64);
    let agent_loop = Arc::new(Scripted {
        events,
        end,
        dropped: Arc::clone(&dropped),
        pace,
    });
    let prepared = Prepared {
        agent_loop,
        messages: vec![user()],
        config: AgentConfig::default(),
        tx,
        rx,
        model: "qwen".to_owned(),
        // A run on no model of this machine's; a test of what a local run
        // is made by resolves one (see `run_made_tests`).
        made_by: gglib_app_services::transcript::MadeBy {
            model: "qwen".to_owned(),
            quantization: None,
            device: None,
            context_size: None,
        },
        local_model: None,
        far_model: None,
        hold: None,
    };
    (prepared, dropped)
}

/// A turn with reasoning and a tool call, then the answer.
pub(super) fn reply() -> Vec<AgentEvent> {
    vec![
        AgentEvent::ReasoningDelta {
            content: "REASON-SECRET".to_owned(),
        },
        AgentEvent::TextDelta {
            content: "Looking.".to_owned(),
        },
        AgentEvent::ToolCallStart {
            tool_call: ToolCall {
                id: "c1".to_owned(),
                name: "read_file".to_owned(),
                arguments: json!({ "path": "ARGUMENT-SECRET" }),
            },
            display_name: "Read File".to_owned(),
            args_summary: None,
        },
        AgentEvent::ToolCallComplete {
            tool_name: "read_file".to_owned(),
            result: ToolResult::text("c1".to_owned(), "RESULT-SECRET".to_owned(), true),
            wait_ms: 0,
            execute_duration_ms: 1,
            display_name: "Read File".to_owned(),
            duration_display: "1ms".to_owned(),
        },
        AgentEvent::IterationComplete {
            iteration: 1,
            tool_calls: 1,
        },
        AgentEvent::TextDelta {
            content: "ANSWER-SECRET".to_owned(),
        },
    ]
}

pub(super) fn finished_reply() -> Vec<AgentEvent> {
    let mut events = reply();
    events.push(AgentEvent::FinalAnswer {
        content: "ANSWER-SECRET".to_owned(),
    });
    events
}

pub(super) async fn conversation(state: &AppState) -> i64 {
    state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap()
}

/// A transcript saved to `conversation`, its message after the rest.
pub(super) fn saving(conversation: i64) -> super::launch::Transcript {
    super::launch::Transcript {
        conversation_id: Some(conversation),
        answer_saved: false,
        remember: None,
    }
}

pub(super) async fn start(
    state: &AppState,
    id: &str,
    conversation: Option<i64>,
    p: Prepared,
) -> RunInfo {
    let created = launch(
        state,
        id,
        LOCAL,
        super::launch::Transcript {
            conversation_id: conversation,
            answer_saved: false,
            remember: None,
        },
        p,
        super::compose::take_permit(state).expect("a free slot"),
    )
    .await
    .expect("started");
    assert!(created.created);
    created.info
}

/// Every run has ended and handled its end.
pub(super) async fn settled(state: &AppState) {
    tokio::time::timeout(Duration::from_secs(5), state.runs.drained())
        .await
        .expect("every run ends within five seconds");
}

pub(super) async fn saved(state: &AppState, conversation: i64) -> Vec<Message> {
    state
        .core
        .chat_history()
        .get_messages(conversation)
        .await
        .unwrap()
}

pub(super) fn meta(row: &Message, key: &str) -> Value {
    row.metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Wait until the run has logged `events` events: its loop is running.
pub(super) async fn logged(state: &AppState, id: &str, events: usize) {
    let events = u32::try_from(events).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.runs.get(&LOCAL, id).unwrap().last_seq < events {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the events are logged within five seconds");
}

/// How many frames a run's stream gives, and the end it closes with.
pub(super) async fn drain(mut events: gglib_core::ports::RunEvents) -> (usize, Option<RunInfo>) {
    use futures_util::StreamExt as _;
    let mut frames = 0;
    let read = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.next().await {
            match event {
                gglib_core::ports::RunEvent::Frame { .. } => frames += 1,
                gglib_core::ports::RunEvent::Preview { .. } => {}
                gglib_core::ports::RunEvent::End(info) => return Some(info),
            }
        }
        None
    })
    .await
    .expect("the stream ends within five seconds");
    (frames, read)
}
