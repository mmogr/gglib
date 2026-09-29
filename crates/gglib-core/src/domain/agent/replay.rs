//! A reply's saved rows, rebuilt from the events it logged.
//!
//! An agent run logs each [`AgentEvent`](super::AgentEvent) as its JSON
//! frame. Once the run ends, whatever the end, those frames are all there is
//! of the reply: the loop's own history is pruned to its context budget and
//! carries no reasoning, and a cancelled or failed loop returns none. So the
//! rows are rebuilt from the frames: one assistant row per model turn (its
//! text, its tool calls, its reasoning) followed by one tool row per result,
//! in the shape [`to_new_message`] gives the CLI's sessions.
//!
//! Only the fields read here are parsed, so a frame this does not know is
//! skipped rather than failing the whole transcript.

use serde::Deserialize;
use serde_json::{Map, Value};

use super::messages::{AgentMessage, AssistantContent};
use super::tool_types::{ToolCall, ToolResult};
use super::transcript::to_new_message;
use crate::domain::chat::NewMessage;

/// The metadata key the chat page reads an assistant row's reasoning from.
pub const THINKING_KEY: &str = "thinking";

/// The metadata key the chat page reads how long a turn thought from, in
/// seconds: from its first reasoning event to its last, as they were logged.
pub const THINKING_DURATION_KEY: &str = "thinkingDurationSeconds";

/// The metadata key set to `true` on the last assistant row of a reply that
/// did not finish: its run was cancelled or failed.
pub const INCOMPLETE_KEY: &str = "incomplete";

/// A tool row's content for a call the reply stopped before answering, so
/// no saved call is left without its answer when the history is sent back.
pub const UNFINISHED_TOOL_CALL: &str = "The tool call did not finish: the reply stopped first.";

/// The parts of a logged event the rows are made from.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Logged {
    TextDelta {
        content: String,
    },
    ReasoningDelta {
        content: String,
    },
    ToolCallStart {
        tool_call: ToolCall,
    },
    ToolCallComplete {
        result: ToolResult,
    },
    IterationComplete {},
    FinalAnswer {
        content: String,
    },
    #[serde(other)]
    Other,
}

/// One model turn as it arrived.
#[derive(Default)]
struct Turn {
    text: String,
    reasoning: String,
    calls: Vec<ToolCall>,
    results: Vec<ToolResult>,
    /// When its first and last reasoning events were logged, in ms.
    reasoned: Option<(u64, u64)>,
}

impl Turn {
    fn is_empty(&self) -> bool {
        self.text.is_empty()
            && self.reasoning.is_empty()
            && self.calls.is_empty()
            && self.results.is_empty()
    }

    /// The assistant row, then a tool row per call in the calls' order: its
    /// result, or [`UNFINISHED_TOOL_CALL`] when none arrived.
    fn into_rows(mut self, conversation_id: i64, rows: &mut Vec<NewMessage>) {
        let assistant = AgentMessage::Assistant {
            content: AssistantContent {
                text: Some(self.text).filter(|t| !t.is_empty()),
                tool_calls: self.calls.clone(),
            },
        };
        let mut row = to_new_message(&assistant, conversation_id);
        if !self.reasoning.is_empty() {
            set(&mut row, THINKING_KEY, Value::String(self.reasoning));
            if let Some((first, last)) = self.reasoned {
                // Tenths of a second, as the page shows them.
                #[allow(clippy::cast_precision_loss)]
                let seconds = (last.saturating_sub(first) / 100) as f64 / 10.0;
                set(&mut row, THINKING_DURATION_KEY, Value::from(seconds));
            }
        }
        rows.push(row);
        for call in &self.calls {
            let result = match self.results.iter().position(|r| r.tool_call_id == call.id) {
                Some(i) => self.results.remove(i),
                None => ToolResult {
                    tool_call_id: call.id.clone(),
                    content: UNFINISHED_TOOL_CALL.to_owned(),
                    success: false,
                },
            };
            rows.push(tool_row(result, conversation_id));
        }
        for result in self.results {
            rows.push(tool_row(result, conversation_id));
        }
    }
}

fn tool_row(result: ToolResult, conversation_id: i64) -> NewMessage {
    let message = AgentMessage::Tool {
        tool_call_id: result.tool_call_id,
        content: result.content,
    };
    to_new_message(&message, conversation_id)
}

/// Set `key` in a row's metadata, making the metadata an object if it had
/// none.
fn set(row: &mut NewMessage, key: &str, value: Value) {
    let mut fields = match row.metadata.take() {
        Some(Value::Object(fields)) => fields,
        _ => Map::new(),
    };
    fields.insert(key.to_owned(), value);
    row.metadata = Some(Value::Object(fields));
}

/// The rows a reply's logged `frames` make, for `conversation_id`.
///
/// `finished` is whether the run completed. When it did not, what arrived
/// of the last turn is kept, and the last assistant row is marked
/// [`INCOMPLETE_KEY`]; a reply that produced nothing at all still gets one
/// empty assistant row, marked, so the conversation says the reply stopped.
pub fn rows_from_frames<'a>(
    frames: impl IntoIterator<Item = &'a str>,
    finished: bool,
    conversation_id: i64,
) -> Vec<NewMessage> {
    rows_from_timed_frames(
        frames.into_iter().map(|f| (f, None)),
        finished,
        conversation_id,
    )
}

/// As [`rows_from_frames`], each frame with when it was logged (ms, any
/// origin), so a turn that reasoned records how long for:
/// [`THINKING_DURATION_KEY`], from its first reasoning event to its last.
pub fn rows_from_timed_frames<'a>(
    frames: impl IntoIterator<Item = (&'a str, Option<u64>)>,
    finished: bool,
    conversation_id: i64,
) -> Vec<NewMessage> {
    let mut rows = Vec::new();
    let mut turn = Turn::default();
    for (frame, at) in frames {
        let Ok(event) = serde_json::from_str::<Logged>(frame) else {
            continue;
        };
        match event {
            Logged::TextDelta { content } => turn.text.push_str(&content),
            Logged::ReasoningDelta { content } => {
                turn.reasoning.push_str(&content);
                if let Some(at) = at {
                    turn.reasoned = Some(turn.reasoned.map_or((at, at), |(first, _)| (first, at)));
                }
            }
            Logged::ToolCallStart { tool_call } => turn.calls.push(tool_call),
            Logged::ToolCallComplete { result } => turn.results.push(result),
            Logged::IterationComplete {} => {
                std::mem::take(&mut turn).into_rows(conversation_id, &mut rows);
            }
            Logged::FinalAnswer { content } => {
                // The answer as the loop settled it, which the deltas that
                // streamed it may not spell exactly.
                turn.text = content;
                std::mem::take(&mut turn).into_rows(conversation_id, &mut rows);
            }
            Logged::Other => {}
        }
    }
    if !turn.is_empty() {
        turn.into_rows(conversation_id, &mut rows);
    }
    if !finished {
        mark_incomplete(&mut rows, conversation_id);
    }
    rows
}

fn mark_incomplete(rows: &mut Vec<NewMessage>, conversation_id: i64) {
    let last = rows
        .iter()
        .rposition(|row| row.role == crate::domain::chat::MessageRole::Assistant);
    let index = last.unwrap_or_else(|| {
        Turn::default().into_rows(conversation_id, rows);
        rows.len() - 1
    });
    set(&mut rows[index], INCOMPLETE_KEY, Value::Bool(true));
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod replay_tests;
