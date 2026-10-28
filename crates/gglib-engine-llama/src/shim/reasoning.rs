//! A reply split into its reasoning and its answer, as the pinned
//! llama-server splits it under its default `--reasoning-format deepseek`
//! (`common/chat-auto-parser-generator.cpp`, `common/chat-peg-parser.cpp`).
//!
//! llama-server parses the reply after its generation prompt: a
//! `<think>…</think>` block at the start is reasoning, sent as
//! `reasoning_content`, and the rest is the answer, sent as `content`.
//! Where the block opens decides where a reply starts. A generation prompt
//! that opens `<think>` (Qwen3.5 thinking) starts the reply in reasoning;
//! one that holds a whole block (thinking off) leaves the reply all answer;
//! any other lets the reply open the block before its first word (Qwen3
//! thinking). A template with no `</think>` has no block, and its reply is
//! all answer, tags and all, as llama-server's content-only parse leaves
//! it. Only a block at the start counts: a `<think>` later in the answer is
//! the answer's.
//!
//! Reasoning that is only whitespace is no reasoning, and the whitespace
//! that joins a tag to its text is dropped. llama-server drops as much of
//! it as its analysis of the template finds in the tags themselves; this
//! drops the whole run. Reasoning keeps its own trailing newline.
//!
//! llama-server's analysis also finds other markers (`<|START_THINKING|>`,
//! `[THINK]`, a block that closes without opening); those are not read
//! here, and such a template's reply is all answer.

use gglib_gguf::ChatTemplate;
use serde_json::Value;

use crate::render::Chat;

const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";

/// The prompt `body` renders to through `template`, and where its reply
/// starts; or why it cannot be rendered.
///
/// `reasoning_format` is read as llama-server reads it: `none` leaves the
/// reply all answer, `auto` and `deepseek` split it, and any other format
/// is refused.
pub(crate) fn rendered(body: &Value, template: &ChatTemplate) -> Result<(String, Start), String> {
    let split = match body.get("reasoning_format") {
        None | Some(Value::Null) => true,
        Some(Value::String(f)) if f == "auto" || f == "deepseek" => true,
        Some(Value::String(f)) if f == "none" => false,
        Some(other) => {
            return Err(format!(
                "'reasoning_format' {other} is not supported by gglib's in-process engine"
            ));
        }
    };
    let chat = Chat::read(body)?;
    let prompt = chat.render(template)?;
    let start = if split {
        Start::of(template.source(), &chat.generation_prompt(template)?)
    } else {
        Start::Answer
    };
    Ok((prompt, start))
}

/// Where a reply starts, read from the prompt it continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Start {
    /// In the answer, and nothing after splits it.
    Answer,
    /// Inside a think block the generation prompt opened.
    Reasoning,
    /// Before a think block the reply may open.
    MayOpen,
}

impl Start {
    /// Where a reply starts under `template`, after `generation_prompt`,
    /// the text `add_generation_prompt` adds to the render.
    pub(crate) fn of(template: &str, generation_prompt: &str) -> Self {
        if !template.contains(CLOSE) {
            return Self::Answer;
        }
        match generation_prompt.rfind(OPEN) {
            Some(at) if generation_prompt[at..].contains(CLOSE) => Self::Answer,
            Some(_) => Self::Reasoning,
            None => Self::MayOpen,
        }
    }
}

/// Text of the reply, as it is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    /// `reasoning_content`.
    Reasoning(String),
    /// `content`.
    Answer(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Before the reply's first word, which may open a block.
    Opening,
    /// Inside the block, until `</think>`.
    Reasoning,
    /// Past `</think>`, before the answer's first word.
    Closed,
    /// The answer.
    Answer,
}

/// Splits a reply as it streams, holding back what may yet be a tag or the
/// whitespace beside one.
#[derive(Debug, Clone)]
pub(crate) struct ReasoningSplit {
    phase: Phase,
    /// Text not sent yet.
    held: String,
    /// Whether any reasoning has been sent.
    reasoned: bool,
}

impl ReasoningSplit {
    /// A split for a reply that starts at `start`.
    pub(crate) fn new(start: Start) -> Self {
        let phase = match start {
            Start::Answer => Phase::Answer,
            Start::Reasoning => Phase::Reasoning,
            Start::MayOpen => Phase::Opening,
        };
        Self {
            phase,
            held: String::new(),
            reasoned: false,
        }
    }

    /// What of the reply, `text` added, can be sent now.
    pub(crate) fn push(&mut self, text: &str) -> Vec<Part> {
        self.held.push_str(text);
        let mut parts = Vec::new();
        loop {
            match self.phase {
                Phase::Answer => {
                    if !self.held.is_empty() {
                        parts.push(Part::Answer(std::mem::take(&mut self.held)));
                    }
                    return parts;
                }
                Phase::Opening => {
                    self.held = self.held.trim_start().to_owned();
                    if let Some(rest) = self.held.strip_prefix(OPEN) {
                        self.held = rest.to_owned();
                        self.phase = Phase::Reasoning;
                    } else if OPEN.starts_with(self.held.as_str()) {
                        return parts;
                    } else {
                        self.phase = Phase::Answer;
                    }
                }
                Phase::Reasoning => {
                    if !self.reasoned {
                        self.held = self.held.trim_start().to_owned();
                    }
                    if let Some(at) = self.held.find(CLOSE) {
                        let rest = self.held.split_off(at + CLOSE.len());
                        self.held.truncate(at);
                        self.reason(&mut parts);
                        self.held = rest;
                        self.phase = Phase::Closed;
                    } else {
                        let keep = self.held.split_off(partial_close(&self.held));
                        self.reason(&mut parts);
                        self.held = keep;
                        return parts;
                    }
                }
                Phase::Closed => {
                    self.held = self.held.trim_start().to_owned();
                    if self.held.is_empty() {
                        return parts;
                    }
                    self.phase = Phase::Answer;
                }
            }
        }
    }

    /// What was held back, once the reply has ended: a tag begun and never
    /// finished is text of the part it was found in.
    pub(crate) fn finish(&mut self) -> Vec<Part> {
        let held = std::mem::take(&mut self.held);
        match self.phase {
            Phase::Opening | Phase::Answer if !held.is_empty() => vec![Part::Answer(held)],
            Phase::Reasoning if !held.is_empty() => vec![Part::Reasoning(held)],
            _ => Vec::new(),
        }
    }

    /// Send what is held as reasoning, if there is any.
    fn reason(&mut self, parts: &mut Vec<Part>) {
        if !self.held.is_empty() {
            self.reasoned = true;
            parts.push(Part::Reasoning(std::mem::take(&mut self.held)));
        }
    }
}

/// Where the longest end of `text` that begins `</think>` starts;
/// `text.len()` when no end does.
fn partial_close(text: &str) -> usize {
    text.char_indices()
        .map(|(i, _)| i)
        .find(|&i| CLOSE.starts_with(&text[i..]))
        .unwrap_or(text.len())
}

#[cfg(test)]
#[path = "reasoning_tests.rs"]
mod reasoning_tests;
