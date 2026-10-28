//! A generation's events turned into llama-server's answer: a stream of
//! chunks, or one completion.
//!
//! A piece of text is cut at a stop string first, as llama-server matches
//! stop strings in the raw reply, then split into reasoning and answer.

use serde_json::Value;

use super::frames::{self, Clock, Envelope, ErrorKind};
use super::reasoning::{Part, ReasoningSplit};
use super::stop::StopScanner;
use crate::decoder::{Abort, FinishReason, Finished, Step};
use crate::engine::{Progress, RunError};

/// What the decode thread sends the reply.
#[derive(Debug)]
pub(crate) enum Event {
    Progress(Progress),
    Done(Result<Finished, RunError>),
}

/// How the client asked to be answered.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Asked {
    /// `stream_options.include_usage`: a last chunk with the counts.
    pub(crate) include_usage: bool,
    /// `return_progress`: a chunk for each step of reading the prompt.
    pub(crate) return_progress: bool,
}

/// One reply as it is assembled.
#[derive(Debug)]
pub(crate) struct Reply {
    envelope: Envelope,
    asked: Asked,
    stops: StopScanner,
    split: ReasoningSplit,
    /// Raised when a stop string ends the reply early.
    abort: Abort,
    clock: Clock,
    prompt_tokens: u32,
    started: bool,
    /// The answer sent, for a client that did not stream.
    text: String,
    /// The reasoning sent, for a client that did not stream.
    reasoning: String,
    /// The `finish_reason`, once the generation has ended.
    reason: Option<&'static str>,
}

/// What one event adds to a reply.
#[derive(Debug)]
pub(crate) enum Out {
    /// Chunks to stream.
    Chunks(Vec<Value>),
    /// The generation ended; these are the last chunks, `[DONE]` to follow.
    End(Vec<Value>),
    /// The generation failed after it started: this error, and nothing
    /// after it.
    Failed(ErrorKind, String),
}

impl Reply {
    pub(crate) fn new(
        model: &str,
        asked: Asked,
        stops: StopScanner,
        split: ReasoningSplit,
        abort: Abort,
    ) -> Self {
        Self {
            envelope: Envelope::new(model),
            asked,
            stops,
            split,
            abort,
            clock: Clock::start(),
            prompt_tokens: 0,
            started: false,
            text: String::new(),
            reasoning: String::new(),
            reason: None,
        }
    }

    /// Take in one event of the generation.
    pub(crate) fn take(&mut self, event: Event) -> Out {
        match event {
            Event::Progress(Progress::Started { prompt_tokens }) => {
                self.prompt_tokens = prompt_tokens;
                self.clock = Clock::start();
                Out::Chunks(Vec::new())
            }
            Event::Progress(Progress::Step(Step::Progress(p))) => {
                let chunks = if self.asked.return_progress {
                    vec![self.envelope.role(Some((p, self.clock.elapsed_ms())))]
                } else {
                    Vec::new()
                };
                Out::Chunks(chunks)
            }
            Event::Progress(Progress::Step(Step::Piece(piece))) => {
                self.clock.token();
                let mut chunks = Vec::new();
                if !self.started {
                    self.started = true;
                    chunks.push(self.envelope.role(None));
                }
                let was_stopped = self.stops.stopped();
                let text = self.stops.push(&piece);
                if self.stops.stopped() && !was_stopped {
                    self.abort.raise();
                }
                let parts = self.split.push(&text);
                chunks.extend(parts.into_iter().map(|part| self.send(part)));
                Out::Chunks(chunks)
            }
            Event::Done(Ok(finished)) => self.end(&finished),
            Event::Done(Err(e)) => Out::Failed(kind_of(&e), message_of(&e)),
        }
    }

    fn end(&mut self, finished: &Finished) -> Out {
        let reason = match (finished.reason, self.stops.stopped()) {
            (_, true) | (FinishReason::Eos, false) => "stop",
            (FinishReason::Length, false) => "length",
            (FinishReason::Aborted, false) => {
                return Out::Failed(
                    ErrorKind::Unavailable,
                    "the generation was aborted before it ended".to_owned(),
                );
            }
        };
        let rest = self.stops.flush();
        let mut parts = self.split.push(&rest);
        parts.extend(self.split.finish());
        let mut chunks: Vec<Value> = parts.into_iter().map(|part| self.send(part)).collect();
        let timings = self.timings(finished);
        if self.asked.include_usage {
            chunks.push(self.envelope.finish(reason, None));
            chunks.push(self.envelope.usage(self.usage(finished), timings));
        } else {
            chunks.push(self.envelope.finish(reason, Some(timings)));
        }
        self.reason = Some(reason);
        Out::End(chunks)
    }

    /// `part` kept for the whole reply, and its chunk.
    fn send(&mut self, part: Part) -> Value {
        match part {
            Part::Reasoning(text) => {
                self.reasoning.push_str(&text);
                self.envelope.reasoning(&text)
            }
            Part::Answer(text) => {
                self.text.push_str(&text);
                self.envelope.content(&text)
            }
        }
    }

    /// The whole reply as one `chat.completion`, once [`Reply::take`] has
    /// taken the generation's end.
    pub(crate) fn completion(&self, finished: &Finished) -> Value {
        self.envelope.completion(
            &self.text,
            &self.reasoning,
            self.reason.unwrap_or("stop"),
            self.usage(finished),
            self.timings(finished),
        )
    }

    fn usage(&self, f: &Finished) -> Value {
        frames::usage(self.prompt_tokens, f.cached_tokens, f.completion_tokens)
    }

    fn timings(&self, f: &Finished) -> Value {
        self.clock
            .timings(self.prompt_tokens, f.cached_tokens, f.completion_tokens)
    }
}

/// The error kind a failed generation is answered with.
pub(crate) fn kind_of(e: &RunError) -> ErrorKind {
    match e {
        RunError::ModelChanged => ErrorKind::Unavailable,
        RunError::OverContext { .. } => ErrorKind::InvalidRequest,
        RunError::Decoder(_) => ErrorKind::Server,
    }
}

/// What a failed generation tells the client.
pub(crate) fn message_of(e: &RunError) -> String {
    match e {
        RunError::ModelChanged => "the model was unloaded before this request ran".to_owned(),
        RunError::OverContext { n_prompt, n_ctx } => {
            format!(
                "request ({n_prompt} tokens) exceeds the available context size ({n_ctx} tokens)"
            )
        }
        RunError::Decoder(e) => e.to_string(),
    }
}
