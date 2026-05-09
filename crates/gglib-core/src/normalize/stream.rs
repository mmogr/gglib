//! [`NormalizingStream`] — the single wrap point that canonicalises an
//! LLM event stream.
//!
//! Adapters that implement [`crate::ports::LlmCompletionPort`] wrap the
//! inner SSE-derived stream **once** with `NormalizingStream::new(inner,
//! get_parser(&model.tags))`.  Every downstream consumer (Axum SSE, CLI,
//! Tauri, the proxy, the agent loop) then sees a strict OpenAI-shaped
//! sequence of [`LlmStreamEvent`] values, regardless of which dialect the
//! underlying model speaks.
//!
//! ## Translation rules
//!
//! - `TextDelta` → routed through [`ToolCallParser::push_text`]; the parser
//!   may strip dialect markup and synthesise [`LlmStreamEvent::ToolCallDelta`]
//!   events for any extracted tool calls.
//! - `ReasoningDelta` → routed through [`ToolCallParser::push_reasoning`]
//!   symmetrically.
//! - `ToolCallDelta` → forwarded unchanged (already conformant).  The
//!   wrapper records the highest seen `index` so synthesised deltas use
//!   non-colliding indices.
//! - `PromptProgress` → forwarded unchanged.
//! - `Done` → [`ToolCallParser::finish`] is called first, any flushed
//!   bytes / tool calls / errors are emitted, **then** `Done` is forwarded
//!   last.  The contract that every stream ends with exactly one `Done`
//!   item is preserved.
//!
//! ## Errors
//!
//! Upstream `Err` items terminate the stream early (we propagate them
//! verbatim).  Non-fatal normalization issues from the parser are surfaced
//! as [`LlmStreamEvent::NormalizationError`] events; they do **not**
//! terminate the stream.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::Result;
use futures_core::Stream;

use super::parser::{ParserOutput, ToolCallParser};
use crate::domain::agent::{LlmStreamEvent, ToolCall};

type InnerStream = Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>;

/// Stream adapter that runs every event through a [`ToolCallParser`] before
/// re-emitting the normalized result.  See module docs.
pub struct NormalizingStream {
    inner: InnerStream,
    parser: Box<dyn ToolCallParser>,
    /// Events ready to emit on the next poll.  A single upstream event can
    /// expand to many downstream events (e.g. `Done` flushes parser state
    /// before propagating).
    queued: VecDeque<LlmStreamEvent>,
    /// Lowest tool-call index that is safe to use for a synthesised delta.
    /// Bumped past every upstream `index` we observe so downstream
    /// collectors can use indices as keys without collision.
    next_index: usize,
    /// `true` once we've forwarded the upstream `Done` (or upstream ended
    /// or errored).  Subsequent polls return `None`.
    terminated: bool,
}

impl NormalizingStream {
    /// Wrap `inner` so every event is normalized through `parser`.
    #[must_use]
    pub fn new(inner: InnerStream, parser: Box<dyn ToolCallParser>) -> Self {
        Self {
            inner,
            parser,
            queued: VecDeque::new(),
            next_index: 0,
            terminated: false,
        }
    }

    /// Translate one parser output batch into the queued event sequence.
    fn enqueue_parser_output(&mut self, mut out: ParserOutput) {
        if !out.forward_text.is_empty() {
            // Strip stray `<think>` / `</think>` boundary tags from text
            // content.  Reasoning models (e.g. Qwen3) send their chain-of-
            // thought in `reasoning_content` SSE fields but leak the closing
            // `</think>` marker into the regular `content` field when
            // transitioning back to output mode.  These tags carry no
            // semantic meaning for the client and produce visible artefacts
            // (e.g. `</think>` appearing verbatim in Zed's chat pane).
            let text = std::mem::take(&mut out.forward_text);
            let text = text.replace("</think>", "").replace("<think>", "");
            if !text.is_empty() {
                self.queued
                    .push_back(LlmStreamEvent::TextDelta { content: text });
            }
        }
        if !out.forward_reasoning.is_empty() {
            self.queued.push_back(LlmStreamEvent::ReasoningDelta {
                content: std::mem::take(&mut out.forward_reasoning),
            });
        }
        for ToolCall {
            id,
            name,
            arguments,
        } in out.tool_calls
        {
            let index = self.next_index;
            self.next_index += 1;
            self.queued.push_back(LlmStreamEvent::ToolCallDelta {
                index,
                id: Some(id),
                name: Some(name),
                arguments: Some(arguments.to_string()),
            });
        }
        for err in out.errors {
            self.queued.push_back(LlmStreamEvent::NormalizationError {
                kind: err.kind,
                raw: err.raw,
            });
        }
    }

    /// Process one upstream event and queue the resulting downstream events.
    fn handle_upstream(&mut self, event: LlmStreamEvent) {
        match event {
            LlmStreamEvent::TextDelta { content } => {
                let out = self.parser.push_text(&content);
                self.enqueue_parser_output(out);
            }
            LlmStreamEvent::ReasoningDelta { content } => {
                let out = self.parser.push_reasoning(&content);
                self.enqueue_parser_output(out);
            }
            LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                if index >= self.next_index {
                    self.next_index = index + 1;
                }
                self.queued.push_back(LlmStreamEvent::ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments,
                });
            }
            LlmStreamEvent::PromptProgress { .. } | LlmStreamEvent::NormalizationError { .. } => {
                self.queued.push_back(event);
            }
            LlmStreamEvent::Done { finish_reason } => {
                let out = self.parser.finish();
                self.enqueue_parser_output(out);
                // Qwen3.5 (and some other models) emit tool_calls in the
                // stream but finish with `finish_reason: "stop"` instead of
                // the required `"tool_calls"`.  Clients such as Zed check
                // finish_reason to decide whether to dispatch tool results;
                // a wrong value causes the conversation to hang.
                let finish_reason = if finish_reason == "stop" && self.next_index > 0 {
                    "tool_calls".to_owned()
                } else {
                    finish_reason
                };
                self.queued
                    .push_back(LlmStreamEvent::Done { finish_reason });
                self.terminated = true;
            }
        }
    }
}

impl Stream for NormalizingStream {
    type Item = Result<LlmStreamEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(ev) = self.queued.pop_front() {
                return Poll::Ready(Some(Ok(ev)));
            }
            if self.terminated {
                return Poll::Ready(None);
            }
            match self.inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(event))) => {
                    self.handle_upstream(event);
                    // Loop to drain `queued` (or poll inner again if empty).
                }
                Poll::Ready(Some(Err(e))) => {
                    self.terminated = true;
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    // Upstream ended without a `Done`.  Flush any held-back
                    // parser state so no bytes are lost, then end.
                    let out = self.parser.finish();
                    self.enqueue_parser_output(out);
                    self.terminated = true;
                    if let Some(ev) = self.queued.pop_front() {
                        return Poll::Ready(Some(Ok(ev)));
                    }
                    return Poll::Ready(None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::{registry::get_parser, tags};
    use std::task::Poll;

    /// Minimal hand-rolled stream that yields a fixed sequence of events.
    struct VecStream {
        items: VecDeque<Result<LlmStreamEvent>>,
    }

    impl VecStream {
        fn new(items: Vec<Result<LlmStreamEvent>>) -> Self {
            Self {
                items: items.into(),
            }
        }
    }

    impl Stream for VecStream {
        type Item = Result<LlmStreamEvent>;
        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.items.pop_front())
        }
    }

    fn drain(mut s: NormalizingStream) -> Vec<LlmStreamEvent> {
        // Poll synchronously with std's no-op waker.  Our test stream is
        // always Ready, so we never observe Pending.
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut out = Vec::new();
        loop {
            match Pin::new(&mut s).poll_next(&mut cx) {
                Poll::Ready(Some(Ok(ev))) => out.push(ev),
                Poll::Ready(Some(Err(e))) => panic!("unexpected error: {e}"),
                Poll::Ready(None) => return out,
                Poll::Pending => panic!("test stream returned Pending"),
            }
        }
    }

    fn wrap(events: Vec<LlmStreamEvent>, qwen: bool) -> NormalizingStream {
        let inner: InnerStream = Box::pin(VecStream::new(events.into_iter().map(Ok).collect()));
        let parser = if qwen {
            get_parser(&[tags::FORMAT_QWEN_XML.to_owned()])
        } else {
            get_parser(&[])
        };
        NormalizingStream::new(inner, parser)
    }

    #[test]
    fn standard_parser_is_passthrough() {
        let events = vec![
            LlmStreamEvent::TextDelta {
                content: "hello".into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ];
        let out = drain(wrap(events.clone(), false));
        assert_eq!(out, events);
    }

    #[test]
    fn qwen_xml_in_text_is_extracted_to_tool_call_delta() {
        let events = vec![
            LlmStreamEvent::TextDelta {
                content: r#"hi <tool_call>{"name":"foo","arguments":{"x":1}}</tool_call> done"#
                    .into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ];
        let out = drain(wrap(events, true));
        // Expect: TextDelta("hi  done"), ToolCallDelta, Done.
        assert_eq!(out.len(), 3);
        assert!(matches!(
            &out[0],
            LlmStreamEvent::TextDelta { content } if content == "hi  done"
        ));
        match &out[1] {
            LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                assert_eq!(*index, 0);
                assert_eq!(id.as_deref(), Some("call_qwen_0"));
                assert_eq!(name.as_deref(), Some("foo"));
                assert_eq!(arguments.as_deref(), Some(r#"{"x":1}"#));
            }
            other => panic!("expected ToolCallDelta, got {other:?}"),
        }
        assert!(matches!(out[2], LlmStreamEvent::Done { .. }));
    }

    #[test]
    fn qwen_xml_in_reasoning_is_extracted_and_text_clean() {
        let events = vec![
            LlmStreamEvent::ReasoningDelta {
                content: r#"think <tool_call>{"name":"foo","arguments":{}}</tool_call> end"#.into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ];
        let out = drain(wrap(events, true));
        assert_eq!(out.len(), 3);
        assert!(matches!(
            &out[0],
            LlmStreamEvent::ReasoningDelta { content } if content == "think  end"
        ));
        assert!(matches!(out[1], LlmStreamEvent::ToolCallDelta { .. }));
        assert!(matches!(out[2], LlmStreamEvent::Done { .. }));
    }

    #[test]
    fn synthesised_index_does_not_collide_with_upstream() {
        let events = vec![
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some("native".into()),
                name: Some("nat".into()),
                arguments: Some("{}".into()),
            },
            LlmStreamEvent::TextDelta {
                content: r#"<tool_call>{"name":"foo","arguments":{}}</tool_call>"#.into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ];
        let out = drain(wrap(events, true));
        // Native delta + synthesised delta + Done = 3.
        assert_eq!(out.len(), 3);
        let LlmStreamEvent::ToolCallDelta { index: idx0, .. } = &out[0] else {
            panic!()
        };
        let LlmStreamEvent::ToolCallDelta { index: idx1, .. } = &out[1] else {
            panic!()
        };
        assert_eq!(*idx0, 0);
        assert_eq!(*idx1, 1);
    }

    #[test]
    fn unclosed_tag_at_done_emits_normalization_error_then_done() {
        let events = vec![
            LlmStreamEvent::TextDelta {
                content: r#"<tool_call>{"name":"foo""#.into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ];
        let out = drain(wrap(events, true));
        // Expect at least: NormalizationError, Done.
        assert!(matches!(out.last(), Some(LlmStreamEvent::Done { .. })));
        assert!(
            out.iter()
                .any(|e| matches!(e, LlmStreamEvent::NormalizationError { .. }))
        );
    }

    #[test]
    fn upstream_ends_without_done_flushes_parser() {
        // No Done at all — wrapper should still terminate cleanly and
        // surface any held-back text.
        let events = vec![LlmStreamEvent::TextDelta {
            content: "<tool".into(),
        }];
        let out = drain(wrap(events, true));
        assert_eq!(out.len(), 1);
        assert!(matches!(
            &out[0],
            LlmStreamEvent::TextDelta { content } if content == "<tool"
        ));
    }

    /// Qwen3.5 emits `tool_calls` in the stream but finishes with
    /// `finish_reason: "stop"` instead of `"tool_calls"`.  The normalizer
    /// must correct this so clients that gate tool dispatch on `finish_reason`
    /// (e.g. Zed) do not hang.
    #[test]
    fn finish_reason_corrected_to_tool_calls_when_tool_calls_seen() {
        let events = vec![
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some("call_0".into()),
                name: Some("read_file".into()),
                arguments: Some(r#"{"path":"/tmp/x"}"#.into()),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(), // wrong — model bug
            },
        ];
        let out = drain(wrap(events, false));
        assert_eq!(out.len(), 2);
        match &out[1] {
            LlmStreamEvent::Done { finish_reason } => {
                assert_eq!(finish_reason, "tool_calls");
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    /// When no tool calls were emitted, `finish_reason: "stop"` must be
    /// left unchanged.
    #[test]
    fn finish_reason_stop_unchanged_when_no_tool_calls() {
        let events = vec![
            LlmStreamEvent::TextDelta {
                content: "hello".into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ];
        let out = drain(wrap(events, false));
        match &out[1] {
            LlmStreamEvent::Done { finish_reason } => {
                assert_eq!(finish_reason, "stop");
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    /// Stray `</think>` closing tags emitted in text content by reasoning
    /// models (e.g. Qwen3) must be stripped before reaching the client.
    #[test]
    fn stray_close_think_tag_stripped_from_text() {
        let events = vec![
            LlmStreamEvent::TextDelta {
                content: "</think>\n\n".into(),
            },
            LlmStreamEvent::TextDelta {
                content: "actual answer".into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ];
        let out = drain(wrap(events, false));
        // First delta should be dropped entirely (only whitespace after stripping).
        // Second delta passes through unchanged.
        let texts: Vec<_> = out
            .iter()
            .filter_map(|e| {
                if let LlmStreamEvent::TextDelta { content } = e {
                    Some(content.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            !texts.iter().any(|t| t.contains("</think>")),
            "found </think> in output: {texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("actual answer")));
    }

    /// `<think>` open tags should also be stripped from text content.
    #[test]
    fn stray_open_think_tag_stripped_from_text() {
        let events = vec![
            LlmStreamEvent::TextDelta {
                content: "<think>spurious</think>real text".into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ];
        let out = drain(wrap(events, false));
        let texts: Vec<_> = out
            .iter()
            .filter_map(|e| {
                if let LlmStreamEvent::TextDelta { content } = e {
                    Some(content.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("<think>") || t.contains("</think>"))
        );
        assert!(texts.iter().any(|t| t.contains("real text")));
    }
}
